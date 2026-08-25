# Python SDK

`sdks/python` — synchronous, blocking client with the full protocol:
WebSocket transport, **automatic X25519 secure session**, tools, game
state, files. Python ≥ 3.7.

```bash
pip install cryptography websocket-client
# in-repo usage:
export PYTHONPATH=$PYTHONPATH:$(pwd)/sdks/python/src
```

```python
from adatp.client import AdaTPClient
```

## Connecting

```python
client = AdaTPClient('127.0.0.1', 3000)                 # host + port
# AdaTPClient(url='wss://rt.example.com/ws')            # full URL
# AdaTPClient('example.com', 3000, path='/ws', secure=True)

client.connect(timeout=10.0)     # opens WS, performs the X25519 handshake
```

## Core API

```python
identity = client.authenticate('user1', 'password123')
# → {'user_id': …, 'username': …, 'role': …}   raises Exception on failure

room = client.join_room('lobby')          # blocks until RoomJoined

client.send_text_message('hi')
text = client.read_text_message()         # next TextMessage (your echo counts)

client.send_file('./report.pdf')          # FileInit → 16 KiB chunks → Complete
client.disconnect()
```

`authenticate`, `join_room` and the typed readers tolerate interleaved
broadcasts — unrelated packets are queued, not lost.

## Game state

```python
client.send_game_state({'v': 1, 'game': 'chess', 'state': state})  # dict → JSON
state = client.read_game_state()          # dict (or bytes for raw payloads)
```

## Tools

```python
tools = client.list_tools()
# [{'name': …, 'description': …, 'schema': …, 'plugin': …}, …]

result = client.call_tool('echo.say', {'text': 'hi', 'uppercase': True})
# → {'echoed': 'HI', 'caller': 'user1'}
# raises Exception("Tool 'x' failed: <code>: <message>") on tool errors
```

Error codes inside the message: `tool_not_found`, `tool_timeout`,
`tool_rate_limited`, `tool_invalid_args`, `tool_failed`, `tool_forbidden`.

## Event loops with `select()`

The client is blocking, but plays fine in a `select` loop (the bundled
terminal chat `example.py` does exactly this):

```python
import select, sys

while True:
    # Buffered packets are invisible to select() — drain them first.
    if client.has_pending():
        handle(client.read_packet())
        continue
    readable, _, _ = select.select([client.socket, sys.stdin], [], [], 0.5)
    if client.socket in readable:
        handle(client.read_packet())
    if sys.stdin in readable:
        client.send_text_message(sys.stdin.readline().strip())
```

`client.socket` is the underlying socket (the client also implements
`fileno()`), `client.has_pending()` reports queued packets.

## Low-level access

```python
pkt = client.read_packet()          # next Packet (inbox first, then wire)
pkt.header.msg_type                 # MessageType IntEnum
pkt.header.session_id               # 16 bytes — sender identity
pkt.payload                         # bytes (encrypted if ENCRYPTED flag)
```

`adatp.protocol` exports `Packet`, `MessageType`, `PacketFlags`,
`HEADER_SIZE`, `MAGIC_NUMBER`; `adatp.crypto.SecureSession` is the cipher
if you need it directly.

## Example programs

`sdks/python/example.py` (terminal chat with `/join`),
`filetransfer_example.py` (send + concurrent receive) — run commands in
the [examples index](examples-index.md).
