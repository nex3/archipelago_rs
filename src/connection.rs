use std::mem;

use crate::{error::*, protocol::*, Client};

mod connection_options;
mod socket;

pub use connection_options::*;
pub(crate) use socket::*;

/// The version of the Archipelago server that this client supports.
const VERSION: NetworkVersion = NetworkVersion {
    major: 0,
    minor: 6,
    build: 0,
    class: String::new(),
};

/// A connection to the Archipelago server. This includes connections that are
/// still being established as well as connections that have been closed.
///
/// This API is designed to make it easy to handle the full life cycle of an
/// Archipelago connection without ever blocking. Because of this, it's safe to
/// use on a game's main thread and easy to run as part of the core game loop.
///
/// The generic type [S] is used to deserialize the slot data in the initial
/// [Connected] message. By default, it will decode the slot data as a
/// dynamically-typed JSON blob.
///
/// This connection is fully non-blocking with the sole exception of the initial
/// DNS lookup, which neither Rust nor the non-blocking library we use supplies
/// a non-blocking interface for.
pub struct Connection<S = serde_json::Value>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    state: ConnectionState<S>,
}

impl<S> Connection<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// Begins a connection to the Archipelago server at [url], with the given
    /// [game] (which must match the apworld's name) and player [name] (which
    /// must match the slot name that was used to generate this session).
    ///
    /// Note that [url] must be an absolute WebSocket URL, including the `ws://`
    /// or `wss://` protocol.
    ///
    /// See [ConnectOptions] for details about optional arguments and their
    /// defaults.
    pub fn new(
        url: impl Into<String>,
        game: impl Into<String>,
        name: impl Into<String>,
        options: ConnectionOptions,
    ) -> Self {
        let url = url.into();
        if !url.starts_with("ws://") && !url.starts_with("wss://") {
            return Connection { state: Error::NoProtocolError(url).into() };
        }

        match Socket::<S>::connect(url) {
            Ok(handshake) => Connection {
                state: ConnectionState::Connecting(Connecting {
                    game: game.into(),
                    state: ConnectingState::SocketConnecting {
                        name: name.into(),
                        options,
                        handshake,
                    },
                }),
            },
            Err(err) => Connection { state: err.into() },
        }
    }

    /// Updates this connection in-place to its next available state.
    ///
    /// This call never blocks, and is expected to be called repeatedly in order
    /// to check for new messages from the underlying connection to Archipelago.
    /// Typically a caller that's integrated Archipelago into a game loop will
    /// call this once each frame, although if the connection was created with
    /// [Connection::new_with_registry] this only needs to be called when
    /// [mio::Poll.poll] emits the token associated with this connection.
    ///
    /// If this results in the connection changing from one state to another,
    /// this returns a [ConnectionStateTransition] describing that transition.
    /// This can be used to notify users that the connection status has changed.
    pub fn update(&mut self) -> Option<ConnectionStateTransition> {
        if let ConnectionState::Connected(ref mut client) = self.state {
            if let Err(err) = client.update() {
                self.state = ConnectionState::Disconnected(err);
                return Some(ConnectionStateTransition {
                    old: ConnectionStateType::Connected,
                    new: ConnectionStateType::Disconnected,
                });
            } else {
                return None;
            }
        } else if matches!(self.state, ConnectionState::Disconnected(_)) {
            return None;
        }

        // Swap in a value so that the state field won't ever be invalid even if
        // Rust unwinds during the connection process. This is necessary because
        // `connecting.update()` moves the old connection.
        let old = self.state.state_type();
        let mut state = Error::ConnectionInterrupted.into();
        mem::swap(&mut self.state, &mut state);
        if let ConnectionState::Connecting(connecting) = state {
            self.state = connecting.update();
            let new = self.state.state_type();
            return if old == new {
                None
            } else {
                Some(ConnectionStateTransition { old, new })
            };
        }

        unreachable!();
    }

    /// The current state of the connection.
    pub fn state(&self) -> &ConnectionState<S> {
        &self.state
    }

    /// The current mutable state of the connection.
    pub fn state_mut(&mut self) -> &mut ConnectionState<S> {
        &mut self.state
    }

    /// The type of the current connection state.
    pub fn state_type(&self) -> ConnectionStateType {
        self.state.state_type()
    }

    /// Converts this into an error that's owned by the caller.
    ///
    /// If this is called when this isn't in an error state, it returns
    /// [Error::NoError].
    pub fn into_err(self) -> Error {
        match self.state {
            ConnectionState::Disconnected(err) => err,
            _ => Error::NoError,
        }
    }
}

pub enum ConnectionState<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// A connection has been requested and is still in the process of being
    /// established.
    Connecting(Connecting<S>),

    /// A connection has been established and fully initialized and the client
    /// is available to use.
    Connected(Client<S>),

    /// The connection has been disconnected either intentionally or due to an
    /// unexpected error.
    Disconnected(Error),
}

impl<S> ConnectionState<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// Returns the [ConnectionStateType] corresponding to this state.
    pub fn state_type(&self) -> ConnectionStateType {
        match self {
            ConnectionState::Connecting(_) => ConnectionStateType::Connecting,
            ConnectionState::Connected(_) => ConnectionStateType::Connected,
            ConnectionState::Disconnected(_) => ConnectionStateType::Disconnected,
        }
    }
}

impl<S, E> From<E> for ConnectionState<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
    E: Into<Error>,
{
    fn from(value: E) -> Self {
        ConnectionState::Disconnected(value.into())
    }
}

/// The state of the Archipelago connection during the initial sequence of
/// protocol handshakes.
pub struct Connecting<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// The name of the game being connected to.
    game: String,

    /// The current state of the handshakes.
    state: ConnectingState<S>,
}

impl<S> Connecting<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// Checks for new non-blocking events and returns the new state
    /// accordingly.
    fn update(mut self) -> ConnectionState<S> {
        use ConnectingState::*;
        match self.state {
            SocketConnecting {
                name,
                options,
                handshake,
            } => match handshake.handshake() {
                Ok(ws) => {
                    log::debug!("Awaiting RoomInfo...");
                    self.state = ApAwaitingRoomInfo { name, options, ws };
                    self.update()
                }
                Err(SocketHandshakeError::WouldBlock(handshake)) => {
                    self.state = SocketConnecting {
                        name,
                        options,
                        handshake,
                    };
                    ConnectionState::Connecting(self)
                }
                Err(SocketHandshakeError::Failure(err)) => err.into(),
            },

            ApAwaitingRoomInfo {
                name,
                options,
                mut ws,
            } => match ws.try_recv() {
                // TODO: cache data packages and only ask for those that are
                // outdated.
                Ok(Some(ServerMessage::RoomInfo(room_info))) => {
                    log::debug!("Awaiting DataPackage...");
                    if let Err(err) = ws.send(ClientMessage::GetDataPackage(GetDataPackage {
                        games: None,
                    })) {
                        return err.into();
                    }
                    self.state = ApAwaitingDataPackage {
                        name,
                        options,
                        ws,
                        room_info,
                    };
                    ConnectionState::Connecting(self)
                }
                Ok(Some(message)) => ProtocolError::UnexpectedResponse {
                    actual: message.type_name(),
                    expected: "RoomInfo",
                }
                .into(),
                Ok(None) => {
                    self.state = ApAwaitingRoomInfo { name, options, ws };
                    ConnectionState::Connecting(self)
                }
                Err(err) => err.into(),
            },

            ApAwaitingDataPackage {
                name,
                options,
                mut ws,
                room_info,
            } => match ws.try_recv() {
                Ok(Some(ServerMessage::DataPackage(DataPackage { data }))) => {
                    log::debug!("Awaiting Connected...");
                    let mut version = VERSION.clone();
                    version.class = "Version".into();
                    if let Err(err) = ws.send(ClientMessage::Connect(Connect {
                        password: options.password,
                        game: self.game.clone(),
                        name,
                        // Specify something useful here if
                        // ArchipelagoMW/Archipelago#998 ever gets sorted out.
                        uuid: "".into(),
                        version: version.clone(),
                        items_handling: options.items_handling.bits(),
                        tags: options.tags,
                        slot_data: options.slot_data,
                    })) {
                        return err.into();
                    }
                    self.state = ApAwaitingConnected {
                        ws,
                        room_info,
                        data_package: data,
                    };
                    ConnectionState::Connecting(self)
                }
                Ok(Some(ServerMessage::ConnectionRefused(ConnectionRefused { errors }))) => {
                    Error::ConnectionRefused(errors.into_iter().map(|e| e.into()).collect()).into()
                }
                Ok(Some(message)) => ProtocolError::UnexpectedResponse {
                    actual: message.type_name(),
                    expected: "RoomInfo",
                }
                .into(),
                Ok(None) => {
                    self.state = ApAwaitingDataPackage {
                        name,
                        options,
                        ws,
                        room_info,
                    };
                    ConnectionState::Connecting(self)
                }
                Err(err) => err.into(),
            },

            ApAwaitingConnected {
                mut ws,
                room_info,
                data_package,
            } => match ws.try_recv() {
                Ok(Some(ServerMessage::Connected(connected))) => {
                    log::info!("Archipelago connection initialized succesfully.");
                    match Client::new(ws, self.game, room_info, data_package, connected) {
                        Ok(client) => ConnectionState::Connected(client),
                        Err(err) => ConnectionState::Disconnected(err),
                    }
                }
                Ok(Some(message)) => ProtocolError::UnexpectedResponse {
                    actual: message.type_name(),
                    expected: "RoomInfo",
                }
                .into(),
                Ok(None) => {
                    self.state = ApAwaitingConnected {
                        ws,
                        room_info,
                        data_package,
                    };
                    ConnectionState::Connecting(self)
                }
                Err(err) => err.into(),
            },
        }
    }
}

/// Various private intermediate states that the connection goes through before
/// it becomes a full Archipelago connection.
enum ConnectingState<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// The underlying socket is connecting.
    SocketConnecting {
        name: String,
        options: ConnectionOptions,
        handshake: MidHandshakeSocket,
    },

    /// The client is waiting for Archipelago to send the `RoomInfo` message.
    ApAwaitingRoomInfo {
        name: String,
        options: ConnectionOptions,
        ws: Socket<S>,
    },

    /// The client is waiting for Archipelago to send the `DataPackage` message.
    ApAwaitingDataPackage {
        name: String,
        options: ConnectionOptions,
        ws: Socket<S>,
        room_info: RoomInfo,
    },

    /// The client is waiting for Archipelago to send the `Connected` message.
    ApAwaitingConnected {
        ws: Socket<S>,
        room_info: RoomInfo,
        data_package: DataPackageObject,
    },
}

/// An enumeration of possible types of [ConnectionState]s, without any extra
/// data attached. Unlike the full [ConnectionState], this implements [Copy] and
/// can be cheaply stored and used to represent state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStateType {
    Connecting,
    Connected,
    Disconnected,
}

/// A struct representing a transition from one state to another. This
/// guarantees that [old] and [new] are always different, and that [old] will
/// always be an earlier state than [new].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionStateTransition {
    pub old: ConnectionStateType,
    pub new: ConnectionStateType,
}
