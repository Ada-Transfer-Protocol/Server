# Node.js SDK

`sdks/node` — TypeScript, CommonJS output in `dist/`. Full-protocol
client: WebSocket transport, **automatic X25519 secure session**, tools,
game state, files.

```bash
cd sdks/node && npm install && npx tsc
```

```js
const { AdaTPClient } = require('adatp');          // packaged
// or from the repo: require('./sdks/node/dist/client.js')
```

Dependencies: [`ws`](https://www.npmjs.com/package/ws), `uuid`. Node ≥ 16
(built-in `crypto` provides X25519/HKDF/AES-GCM).

## Connecting

```js
const client = new AdaTPClient('127.0.0.1', 3000);            // host + port
// or: new AdaTPClient('wss://rt.example.com/ws')             // full URL
// or: new AdaTPClient('example.com', 3000, { path: '/ws', secure: true })

await client.connect();          // opens WS, performs the X25519 handshake
```

After `connect()` resolves, every packet the client sends is AES-256-GCM
encrypted; the server answers in kind.

## Core API

```js
const identity = await client.authenticate('user1', 'password123');
// → { user_id, username, role }        throws Error on AuthFailure

const room = await client.joinRoom('lobby');   // resolves with the room name

await client.sendTextMessage('hi');            // encrypted TextMessage
const text = await client.readNextTextMessage();

client.setMessageHandler((senderHex, text) => { … });  // async chat traffic

await client.sendFile('./report.pdf');   // FileInit → 16 KiB chunks → Complete
await client.disconnect();               // sends Disconnect, closes the WS
client.getSessionId();                   // your identity, hex
```

`authenticate` and `joinRoom` are safe against interleaved broadcasts —
they wait for the right packet *types* and queue everything else.

## Game state

```js
await client.sendGameState({ v: 1, game: 'chess', state });   // JSON-encoded
client.setGameStateHandler((senderHex, state) => { … });      // push style
const state = await client.readNextGameState();               // pull style
```

`Buffer` payloads pass through raw. Semantics: [game state](game-state.md).

## Tools

```js
const tools = await client.listTools();
// [{ name, description, schema, plugin }, …]  — includes system.list_tools

try {
    const result = await client.callTool('echo.say', { text: 'hi', uppercase: true });
    // → { echoed: 'HI', caller: 'user1' }
} catch (e) {
    console.error(e.code, e.message);   // e.code ∈ tool_not_found, tool_timeout,
}                                       //   tool_rate_limited, tool_invalid_args,
                                        //   tool_failed, tool_forbidden
```

`callTool(tool, args = {}, timeoutMs = 15000)` correlates by id, so calls
can overlap freely with chat traffic. Details:
[tools & plugins](tools-and-plugins.md).

## Low-level access

```js
const pkt = await client.readNextPacket();                    // anything next
const pkt2 = await client.readNextPacketOfType(
    [MessageType.PresenceUpdate], 5000);                       // typed wait
```

`Packet` objects expose `header` (`msgType`, `sequence`, `sessionId`, …)
and `payload` (`Buffer`, still encrypted if `flags & Encrypted` — the
convenience methods decrypt for you). Exports from `dist/protocol.js`:
`MessageType`, `PacketFlags`, `Codec`, `MAGIC_NUMBER`, `HEADER_SIZE`.

## Error behavior

- `connect()` rejects on socket errors or a failed handshake.
- `authenticate()` throws `Error('Authentication failed: …')` — after three
  bad attempts the server closes the connection.
- `callTool()` rejects with `.code` set (see above) or a local timeout.
- A dead connection surfaces as rejected sends (`WebSocket is not open`).
  Reconnection strategy is yours; back off and re-run the whole
  connect → authenticate → join sequence.

## Complete example programs

`sdks/node/example.js`, `example.ts`, `filetransfer_example.ts` — run
commands in the [examples index](examples-index.md).
