## 2.0.0

* **Breaking change:** Remove the `Group` type, the `Client::groups()` method,
  and the `Client::teammate_groups()` method.

* Add a `Player::group_members()` method. A player is now considered to be a
  group if this returns a non-empty slice. This more closely matches the
  Archipelago protocol's data model.

* **Breaking change:** Remove `ConnectionOptions::no_slot_data()`. The client
  will now automatically avoid fetching slot data if the `S` type parameter to
  `Client` or `Connection` is `()`.

* **Breaking change:** Add a `'static` bound to the `S` type parameter to
  `Client` or `Connection` is `()`.

* Sanitize game names and checksums before using them as file paths. This fixes
  an error where certain games' data packages were never cached.

* Avoid a possible edge-case bug where the connection could time out if the
  client received a large number of messages and processed each one slowly.

## 1.0.0

* Initial stable release.
