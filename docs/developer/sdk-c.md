# C SDK

`sdks/c` — C11 client library for native and embedded-Linux hosts.
Dependency-free RFC 6455 WebSocket client built in; crypto via OpenSSL
(X25519, HKDF-SHA256, AES-256-GCM).

## Build

```bash
cd sdks/c
cmake -B build -DCMAKE_BUILD_TYPE=Release     # needs OpenSSL (libssl-dev)
cmake --build build
# → build/libadatp.dylib|.so  +  build/adatp_example
```

Link your program with `-ladatp` and include `include/adatp.h`.

**Transport:** `ws://` only. For TLS (`wss://`) terminate at a reverse
proxy / load balancer in front of the server — the C SDK targets trusted
or proxied networks.

## Session lifecycle

```c
#include "adatp.h"

adatp_client_t* c = adatp_client_create("127.0.0.1", 3000);  // port ≤0 → 3000
if (adatp_client_connect(c) != 0) { /* WS + X25519 handshake failed */ }

if (adatp_client_authenticate(c, "cbot", "secret_password") != 0) { /* rejected */ }
if (adatp_client_join_room(c, "lobby") != 0) { /* join failed */ }

adatp_client_send_text(c, "hello from C");

adatp_client_disconnect(c);      // sends Disconnect, closes the socket
adatp_client_destroy(c);
```

All post-connect traffic is encrypted automatically. Return convention:
`0` success, negative on failure (auth: `-4` = credentials rejected).

## Receiving

```c
adatp_packet_t pkt;
if (adatp_client_read_packet(c, &pkt) == 0) {          // one WS message = one packet
    if (pkt.header.msg_type == ADATP_MSG_TEXT_MESSAGE) {
        uint8_t plain[1024];
        int n = adatp_client_decrypt_packet(c, &pkt, plain);   // -1 on tamper
        if (n >= 0) { plain[n] = 0; printf("< %s\n", plain); }
    }
    free(pkt.payload);          // ← ownership rule: payload is malloc'd,
}                               //   the caller frees it (NULL-safe on empty)
```

**Memory rules:** `adatp_client_read_packet` allocates `pkt.payload`
(NULL when the payload is empty) — free it after use. `auth_tag` lives
inline in the struct. Everything you pass *in* is copied before return.

## Generic sends

```c
// Any routable type, encrypted:
adatp_client_send(c, ADATP_MSG_GAME_STATE,
                  (const uint8_t*)"{\"v\":1,\"state\":{}}", 18);
```

Constants cover the full v1 registry: `ADATP_MSG_TEXT_MESSAGE`,
`ADATP_MSG_GAME_STATE` (0x0050), `ADATP_MSG_TOOL_CALL/RESULT/ERROR`
(0x0070-72), `ADATP_MSG_FILE_*`, `ADATP_MSG_PING/PONG` (0x0080/81),
`ADATP_MSG_JOIN_ROOM/ROOM_JOINED`, `ADATP_MSG_DISCONNECT`. Tool calls at
the C level mean sending the [ToolCall JSON](tools-and-plugins.md)
yourself and matching the reply id.

## `select()` integration

```c
int fd = adatp_client_get_socket(c);      // underlying socket fd
fd_set rfds; FD_ZERO(&rfds);
FD_SET(fd, &rfds); FD_SET(STDIN_FILENO, &rfds);
select(fd + 1, &rfds, NULL, NULL, NULL);
if (FD_ISSET(fd, &rfds)) { /* adatp_client_read_packet(...) */ }
```

The bundled `examples/example.c` is a complete terminal chat client
multiplexing stdin and the socket this way (`/join <room>`, `/quit`), and
`filetransfer_example.c` sends + receives files. Run commands:
[examples index](examples-index.md).

## Caveats

- Little-endian hosts assumed (x86/ARM); the wire is LE.
- Blocking I/O; `read_packet` waits for a complete message. Use `select`
  for readiness, and expect WS control frames to be handled internally.
- `join_room`/`authenticate` discard unrelated packets that arrive while
  they wait for their confirmation (fine at connection setup; do the
  room join before subscribing to busy traffic).
