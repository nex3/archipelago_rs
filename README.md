# `archipelago_rs`

A Rust client library that implements the [Archipelago network protocol]. This
is primarily intended for use in integrating games into the [Archipelago
multiworld randomizer], although it could also be used for making Archipelago
bots or other tools.

[Archipelago network protocol]: https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md
[Archipelago multiworld randomizer]: https://archipelago.gg

## Design

This library uses a non-blocking style. The caller must manually poll for new
events using `Connection.update` or `Client.update`. In exchange, the client
never blocks on the network connection and so it can safely be used within a
game's main loop without having to be run on a separate thread.

In most cases, messages from the server are surfaced to the caller as `Event`
structs returned from `Connection.update`. However, when the client makes
requests to the server that have specific responses associated with them (as in
`Client.scout_locations()` or `Client.get()`), this will return a
[`oneshot::Receiver`] that the caller can use to access the response once it's
available.

[`oneshot::Receiver`]: https://docs.rs/oneshot/0.1.11/oneshot/struct.Receiver.html

This library takes responsibility for tracking the state of the server as it
updates and exposing it through getters such as `Client.hint_points()` and
`Client.checked_locations()`. It will emit an `Event::Updated` whenever any of
its fields updates because of a change on the server.

### Interned Strings

This uses the [`ustr`] string interning library. These strings are "interned",
meaning that their data is stored in global storage and they're represented as
opaque scalar values. This means that their memory is never freed, but their
lifetimes don't need to be managed and comparison and hashing operations are
very inexpensive.

[`ustr`]: https://docs.rs/ustr/latest/ustr/

This uses `Ustr`s for strings that don't change either during the lifetime of
the client or across multiple client connections to the same Archipelago room.
This includes the names of games, players (but *not* their aliases), items, and
locations. This makes it possible to share these as copyable objects without
having to worry about lifetimes. It's unlikely that the memory will leak in
practice since it's expected that the client will last for the whole game
session (or, if it disconnects, another client will reconnect to the same room).

### TLS/SSL Support

Rust's support for TLS, especially through WebSockets, is very low-level.
Because the official Archipelago server requires TLS, this package intends to
make using it as simple and batteries-included as possible.

The Rust ecosystem has two broad options for TLS support: the [`rustls` crate],
which implements the entire TLS stack in Rust; and the [`native-tls` crate],
which relies on the operating system's built-in TLS implementation. These each
have their benefits: `rustls` isn't subject to OS-specific quirks and bugs,
while `native-tls` can keep the algorithm and especially the certificate store
up-to-date with OS updates.

[`rustls` crate]: https://crates.io/crates/rustls
[`native-tls` crate]: https://crates.io/crates/native-tls

To ensure maximum compatibility, by default this crate supports both. When
connecting over TLS, it will first attempt to connect using `rustls`, because
this is known to work with archipelago.gg on all platforms (even Wine/Proton, on
which `native-tls` is [currently buggy]). If this fails, it will fall back to
trying `native-tls` instead. If that fails as well, it'll try an unencrpyted TCP
connection.

[currently buggy]: https://github.com/rust-native-tls/rust-native-tls/issues/375

You can control this behavior using crate features. If you enable only one of
the `rustls` or `native-tls` features, this will only connect using that system.
If you enable neither, it will only connect using an unencrypted WebSocket
connection.

## Usage

There are two primary entrypoints for the library. The `Connection` struct is
recommended for users who connect to Archipelago as part of a running game,
which may disconnect and reconnect over time. It encapsulates the lifetime of a
connection, including the time when it's still in the process of connecting and
the time after it's disconnected. This makes it easy for the caller to display
the current status and change its behavior based on whether a connection exists
or not.

If a caller wants to manage the lifetime more directly, they can instead call
`Client::new()` directly. This returns a future that resolves to a `Client`,
which they can then call `Client.update()` on to update its status. This may be
more desirable for tools that only ever intend to connect one time outside of
the context of a game loop.

[See `text_client`] for a simple example of how to use this client in the
context of a main loop.

[See `text_client`]: https://github.com/nex3/archipelago_rs/blob/main/examples/text_client.rs
