use std::time::SystemTime;

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, Stream, StreamExt,
};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tungstenite::protocol::Message;

use crate::protocol::*;

mod death_link_options;

pub use death_link_options::*;

#[derive(Error, Debug)]
pub enum ArchipelagoError {
    #[error("illegal response")]
    IllegalResponse {
        expected: &'static str,
        received: &'static str,
    },
    #[error("connection closed by server")]
    ConnectionClosed,
    #[error("data failed to serialize ({0})")]
    FailedSerialize(#[from] serde_json::Error),
    #[error("failed to deserialize server data ({error})\n{json}")]
    FailedDeserialize {
        json: String,
        error: serde_json::Error,
    },
    #[error("unexpected non-text result from websocket")]
    NonTextWebsocketResult(Message),
    #[error("network error")]
    NetworkError(#[from] tungstenite::Error),
}

/// The client that talks to the Archipelago server using the Archipelago
/// protocol.
///
/// The generic type [S] is used to deserialize the slot data in the initial
/// [Connected] message. By default, it will decode the slot data as a dynamic
/// JSON blob.
pub struct ArchipelagoClient<S = serde_json::Value>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    room_info: RoomInfo,
    message_buffer: Vec<ServerMessage<S>>,
    data_package: Option<DataPackageObject>,
}

impl<S> ArchipelagoClient<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /**
     * Create an instance of the client and connect to the server on the given URL
     */
    pub async fn new(url: &str) -> Result<ArchipelagoClient<S>, ArchipelagoError> {
        // Attempt WSS, downgrade to WS if the TLS handshake fails
        let mut wss_url = String::new();
        wss_url.push_str("wss://");
        wss_url.push_str(url);
        let (mut ws, _) = match connect_async(&wss_url).await {
            Ok(result) => result,
            Err(tungstenite::error::Error::Tls(_)) => {
                let mut ws_url = String::new();
                ws_url.push_str("ws://");
                ws_url.push_str(url);
                connect_async(&ws_url).await?
            }
            Err(error) => return Err(ArchipelagoError::NetworkError(error)),
        };

        let response = recv_messages(&mut ws)
            .await
            .ok_or(ArchipelagoError::ConnectionClosed)??;
        let mut iter = response.into_iter();
        let room_info = match iter.next() {
            Some(ServerMessage::RoomInfo(room)) => room,
            Some(received) => return Err(Self::illegal_response("RoomInfo", received)),
            None => return Err(ArchipelagoError::ConnectionClosed),
        };

        Ok(ArchipelagoClient {
            ws,
            room_info,
            message_buffer: iter.collect(),
            data_package: None,
        })
    }

    /**
     * Create an instance of the client and connect to the server, fetching the given games' Data
     * Package
     */
    pub async fn with_data_package(
        url: &str,
        games: Option<Vec<String>>,
    ) -> Result<ArchipelagoClient<S>, ArchipelagoError> {
        let mut client = Self::new(url).await?;
        client
            .send(ClientMessage::GetDataPackage(GetDataPackage { games }))
            .await?;
        let response = client.recv().await?;
        match response {
            Some(ServerMessage::DataPackage(pkg)) => client.data_package = Some(pkg.data),
            Some(received) => return Err(Self::illegal_response("DataPackage", received)),
            None => return Err(ArchipelagoError::ConnectionClosed),
        }

        Ok(client)
    }

    pub fn room_info(&self) -> &RoomInfo {
        &self.room_info
    }

    pub fn data_package(&self) -> Option<&DataPackageObject> {
        self.data_package.as_ref()
    }

    pub async fn send(&mut self, message: ClientMessage) -> Result<(), ArchipelagoError> {
        let request = serde_json::to_string(&[message])?;
        self.ws.send(Message::Text(request.into())).await?;

        Ok(())
    }

    /**
     * Read a message from the server
     *
     * Will buffer results locally, and return results from buffer or wait on network
     * if buffer is empty
     */
    pub async fn recv(&mut self) -> Result<Option<ServerMessage<S>>, ArchipelagoError> {
        if let Some(message) = self.message_buffer.pop() {
            return Ok(Some(message));
        }
        let messages = recv_messages(&mut self.ws).await;
        if let Some(result) = messages {
            let mut messages = result?;
            messages.reverse();
            let first = messages.pop();
            self.message_buffer = messages;
            Ok(first)
        } else {
            Ok(None)
        }
    }

    /**
     * Send a connect request to the Archipelago server
     *
     * Will attempt to read a Connected packet in response, and will return an error if
     * another packet is found
     */
    pub async fn connect(
        &mut self,
        game: &str,
        name: &str,
        password: Option<&str>,
        items_handling: ItemsHandlingFlags,
        tags: Vec<String>,
    ) -> Result<Connected<S>, ArchipelagoError> {
        self.send(ClientMessage::Connect(Connect {
            game: game.to_string(),
            name: name.to_string(),
            uuid: "".to_string(),
            password: password.map(|p| p.to_string()),
            version: network_version(),
            items_handling: items_handling.bits(),
            tags,
            slot_data: true,
        }))
        .await?;
        let response = self
            .recv()
            .await?
            .ok_or(ArchipelagoError::ConnectionClosed)?;

        match response {
            ServerMessage::Connected(connected) => Ok(connected),
            received => Err(Self::illegal_response("Connected", received)),
        }
    }

    /**
     * Basic chat command which sends text to the server to be distributed to other clients.
     */
    pub async fn say(&mut self, message: &str) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::Say(Say {
                text: message.to_string(),
            }))
            .await?)
    }

    /**
     * Sent to server to request a ReceivedItems packet to synchronize items.
     *
     * Will buffer any non-ReceivedItems packets returned
     */
    pub async fn sync(&mut self) -> Result<ReceivedItems, ArchipelagoError> {
        self.send(ClientMessage::Sync).await?;
        while let Some(response) = self.recv().await? {
            match response {
                ServerMessage::ReceivedItems(items) => return Ok(items),
                resp => self.message_buffer.push(resp),
            }
        }

        Err(ArchipelagoError::ConnectionClosed)
    }

    /**
     * Sent to server to inform it of locations that the client has checked.
     *
     * Used to inform the server of new checks that are made, as well as to sync state.
     */
    pub async fn location_checks(&mut self, locations: Vec<i64>) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::LocationChecks(LocationChecks { locations }))
            .await?)
    }

    /**
     * Sent to the server to inform it of locations the client has seen, but not checked.
     *
     * Useful in cases in which the item may appear in the game world, such as 'ledge items' in A Link to the Past. Non-LocationInfo packets will be buffered
     */
    pub async fn location_scouts(
        &mut self,
        locations: Vec<i64>,
        create_as_hint: u8,
    ) -> Result<LocationInfo, ArchipelagoError> {
        self.send(ClientMessage::LocationScouts(LocationScouts {
            locations,
            create_as_hint,
        }))
        .await?;
        while let Some(response) = self.recv().await? {
            match response {
                ServerMessage::LocationInfo(items) => return Ok(items),
                resp => self.message_buffer.push(resp),
            }
        }

        Err(ArchipelagoError::ConnectionClosed)
    }

    /**
     * Sent to the server to update on the sender's status.
     *
     * Examples include readiness or goal completion. (Example: defeated Ganon in A Link to the Past)
     */
    pub async fn status_update(&mut self, status: ClientStatus) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::StatusUpdate(StatusUpdate { status }))
            .await?)
    }

    /**
     * Send this message to the server, tell it which clients should receive the message and the server will forward the message to all those targets to which any one requirement applies.
     */
    pub async fn bounce(
        &mut self,
        games: Option<Vec<String>>,
        slots: Option<Vec<String>>,
        tags: Vec<String>,
        data: serde_json::Value,
    ) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::Bounce(Bounce {
                games,
                slots,
                tags,
                data: BounceData::Generic(data),
            }))
            .await?)
    }

    /// Sends a death link notification to the server.
    pub async fn death_link(
        &mut self,
        source: String,
        options: DeathLinkOptions,
    ) -> Result<(), ArchipelagoError> {
        self.send(ClientMessage::Bounce(Bounce {
            games: options.games,
            slots: options.slots,
            tags: vec![],
            data: BounceData::DeathLink(DeathLink {
                time: options.time.unwrap_or_else(SystemTime::now),
                cause: options.cause,
                source,
            }),
        }))
        .await
    }

    /**
     * Used to request a single or multiple values from the server's data storage, see the Set package for how to write values to the data storage.
     *
     * A Get package will be answered with a Retrieved package. Non-Retrieved responses are
     * buffered
     */
    pub async fn get(&mut self, keys: Vec<String>) -> Result<Retrieved, ArchipelagoError> {
        self.send(ClientMessage::Get(Get { keys })).await?;
        while let Some(response) = self.recv().await? {
            match response {
                ServerMessage::Retrieved(items) => return Ok(items),
                resp => self.message_buffer.push(resp),
            }
        }

        Err(ArchipelagoError::ConnectionClosed)
    }

    /**
     * Used to write data to the server's data storage, that data can then be shared across worlds or just saved for later.
     *
     * Values for keys in the data storage can be retrieved with a Get package, or monitored with a SetNotify package. Non-SetReply responses are buffered
     */
    pub async fn set(
        &mut self,
        key: String,
        default: serde_json::Value,
        want_reply: bool,
        operations: Vec<DataStorageOperation>,
    ) -> Result<SetReply, ArchipelagoError> {
        self.send(ClientMessage::Set(Set {
            key,
            default,
            want_reply,
            operations,
        }))
        .await?;
        while let Some(response) = self.recv().await? {
            match response {
                ServerMessage::SetReply(items) => return Ok(items),
                resp => self.message_buffer.push(resp),
            }
        }

        Err(ArchipelagoError::ConnectionClosed)
    }

    /**
     * Split the client into two parts, one to handle sending and one to handle receiving.
     *
     * This removes access to a few convenience methods (like `get` or `set`) because it's
     * there's now extra coordination required to match a read and write, but it brings
     * the benefits of allowing simultaneous reading and writing.
     */
    pub fn split(self) -> (ArchipelagoClientSender, ArchipelagoClientReceiver<S>) {
        let Self {
            ws,
            room_info,
            message_buffer,
            data_package,
        } = self;
        let (send, recv) = ws.split();
        (
            ArchipelagoClientSender { ws: send },
            ArchipelagoClientReceiver {
                ws: recv,
                room_info,
                message_buffer,
                data_package,
            },
        )
    }

    /// Returns an illegal response error indicating the [expected] response
    /// type and the actual type of [received].
    fn illegal_response(expected: &'static str, received: ServerMessage<S>) -> ArchipelagoError {
        ArchipelagoError::IllegalResponse {
            expected,
            received: received.type_name(),
        }
    }
}

/**
 * Once split, this struct handles the sending-side of your connection
 *
 * For helper method docs, see ArchipelagoClient. Helper methods that require
 * both sending and receiving are intentionally unavailable; for those messages,
 * use `send`.
 */
pub struct ArchipelagoClientSender {
    ws: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

impl ArchipelagoClientSender {
    pub async fn send(&mut self, message: ClientMessage) -> Result<(), ArchipelagoError> {
        let request = serde_json::to_string(&[message])?;
        self.ws.send(Message::Text(request.into())).await?;

        Ok(())
    }

    pub async fn say(&mut self, message: &str) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::Say(Say {
                text: message.to_string(),
            }))
            .await?)
    }

    pub async fn location_checks(&mut self, locations: Vec<i64>) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::LocationChecks(LocationChecks { locations }))
            .await?)
    }

    pub async fn status_update(&mut self, status: ClientStatus) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::StatusUpdate(StatusUpdate { status }))
            .await?)
    }

    pub async fn bounce(
        &mut self,
        games: Option<Vec<String>>,
        slots: Option<Vec<String>>,
        tags: Vec<String>,
        data: serde_json::Value,
    ) -> Result<(), ArchipelagoError> {
        Ok(self
            .send(ClientMessage::Bounce(Bounce {
                games,
                slots,
                tags,
                data: BounceData::Generic(data),
            }))
            .await?)
    }
}

/**
 * Once split, this struct handles the receiving-side of your connection
 *
 * For helper method docs, see ArchipelagoClient. Helper methods that require
 * both sending and receiving are intentionally unavailable; for those messages,
 * use `recv`.
 */
pub struct ArchipelagoClientReceiver<S = serde_json::Value>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    ws: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    room_info: RoomInfo,
    message_buffer: Vec<ServerMessage<S>>,
    data_package: Option<DataPackageObject>,
}

impl<S> ArchipelagoClientReceiver<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    pub async fn recv(&mut self) -> Result<Option<ServerMessage<S>>, ArchipelagoError> {
        if let Some(message) = self.message_buffer.pop() {
            return Ok(Some(message));
        }
        let messages = recv_messages(&mut self.ws).await;
        if let Some(result) = messages {
            let mut messages = result?;
            messages.reverse();
            let first = messages.pop();
            self.message_buffer = messages;
            Ok(first)
        } else {
            Ok(None)
        }
    }

    pub fn room_info(&self) -> &RoomInfo {
        &self.room_info
    }

    pub fn data_package(&self) -> Option<&DataPackageObject> {
        self.data_package.as_ref()
    }
}

async fn recv_messages<S>(
    mut ws: impl Stream<Item = Result<Message, tungstenite::error::Error>> + std::marker::Unpin,
) -> Option<Result<Vec<ServerMessage<S>>, ArchipelagoError>>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    loop {
        match ws.next().await? {
            Ok(Message::Text(response)) => {
                return Some(
                    serde_json::from_str::<Vec<ServerMessage<S>>>(&response).map_err(|e| {
                        ArchipelagoError::FailedDeserialize {
                            json: response.to_string(),
                            error: e,
                        }
                    }),
                )
            }
            Ok(Message::Close(_)) => return Some(Err(ArchipelagoError::ConnectionClosed)),
            // Ignore pings and pongs. Tungstenite handles these for us but doesn't
            // hide them.
            Ok(Message::Ping(_) | Message::Pong(_)) => (),
            Ok(msg) => return Some(Err(ArchipelagoError::NonTextWebsocketResult(msg))),
            Err(e) => return Some(Err(e.into())),
        }
    }
}
