## 1.1.0

* Add a threaded mode in which the WebSocket connection runs in a background
  thread using blocking IO. This can help ensure a reliable connection,
  particularly under WINE where non-blocking mode seems to produce spurious
  disconnections. Non-blocking mode is used by default except under WINE.

* Add a `ConnectionOptions::mode()` setter, which allows the caller
  to control whether the client runs in threaded mode.

## 1.0.0

* Initial stable release
