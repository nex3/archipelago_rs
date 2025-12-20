use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use serde::de::DeserializeOwned;
use smol::future::FutureExt;

use crate::{Client, ConnectionOptions, Event, error::*};

mod socket;

pub(crate) use socket::*;

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
#[derive(Default)]
// TODO: Use TAITs to avoid boxing the connection future and thus avoid
// `'static` here.
pub struct Connection<S: DeserializeOwned + 'static = serde_json::Value> {
    /// The current state of the connection.
    state: ConnectionState<S>,
}

impl<S: DeserializeOwned + 'static> Connection<S> {
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
        Connection {
            state: ConnectionState::Connecting(Connecting(Box::pin(Client::connect(
                url.into(),
                game.into(),
                name.into(),
                options,
            )))),
        }
    }

    /// Updates this connection in-place to its next available state.
    ///
    /// This call never blocks, and is expected to be called repeatedly in order
    /// to check for new messages from the underlying connection to Archipelago.
    /// Typically a caller that's integrated Archipelago into a game loop will
    /// call this once each frame.
    ///
    /// This returns any events that were received from the server since the
    /// last time this was called. If the connection encounters a fatal error,
    /// [Event::Error] will be [Error::Elsewhere] and the actual error will be
    /// available from [state] or [into_err].
    ///
    /// Most errors are fatal, but some (specifically [Error::ProtocolError]s)
    /// are recoverable. If the connection encounters a recoverable error, it
    /// will remain in [ConnectionState::Connected] and continue emitting events
    /// afterwards.
    pub fn update(&mut self) -> Vec<Event> {
        match self.state {
            ConnectionState::Connecting(Connecting(ref mut future)) => match try_future(future) {
                Some(Ok(client)) => {
                    self.state = ConnectionState::Connected(client);
                    // It's unlikely that any events have come in already, but
                    // no harm in checking.
                    let later_events = self.update();
                    let mut events = Vec::with_capacity(later_events.len() + 1);
                    events.push(Event::Connected);
                    events.extend(later_events);
                    events
                }
                Some(Err(err)) => {
                    self.state = ConnectionState::Disconnected(err);
                    vec![Event::Error(Error::Elsewhere)]
                }
                None => vec![],
            },
            ConnectionState::Connected(ref mut client) => {
                let mut events = client.update();
                if let Some(Event::Error(error)) =
                    events.pop_if(|e| matches!(e, Event::Error(err) if err.is_fatal()))
                {
                    self.state = ConnectionState::Disconnected(error);
                    events.push(Event::Error(Error::Elsewhere));
                }
                events
            }
            ConnectionState::Disconnected(_) => vec![],
        }
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
    /// [Error::ClientDisconnected].
    pub fn into_err(self) -> Error {
        match self.state {
            ConnectionState::Disconnected(err) => err,
            _ => Error::ClientDisconnected,
        }
    }
}

pub enum ConnectionState<S: DeserializeOwned + 'static> {
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

impl<S: DeserializeOwned + 'static> ConnectionState<S> {
    /// Returns the [ConnectionStateType] corresponding to this state.
    pub fn state_type(&self) -> ConnectionStateType {
        match self {
            ConnectionState::Connecting(_) => ConnectionStateType::Connecting,
            ConnectionState::Connected { .. } => ConnectionStateType::Connected,
            ConnectionState::Disconnected(_) => ConnectionStateType::Disconnected,
        }
    }
}

impl<S: DeserializeOwned + 'static> Default for ConnectionState<S> {
    /// The default connection state is disconnected with
    /// [Error::ClientDisconnected].
    fn default() -> Self {
        ConnectionState::Disconnected(Error::ClientDisconnected)
    }
}

/// The state of the Archipelago connection during the initial sequence of
/// protocol handshakes.
pub struct Connecting<S: DeserializeOwned>(Pin<Box<dyn Future<Output = Result<Client<S>, Error>>>>);

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

/// If [future] is complete, returns its value. Otherwise, returns `None`.
///
/// If this returns a value, [future] must not be polled again afterwards.
fn try_future<T, F: FutureExt<Output = T> + Unpin>(future: &mut F) -> Option<T> {
    let mut context = Context::from_waker(Waker::noop());
    match future.poll(&mut context) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}
