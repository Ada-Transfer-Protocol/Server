# Getting Started

Ten minutes from zero to a chat roundtrip. Everything below uses the demo
credentials from `server/server/users.json` (`user1` / `password123`) —
**development only**; production uses the `api` auth driver
([production docs](../production/README.md)).

## 1. Run the server

### Option A — cargo (native)

```bash
git clone https://github.com/Ada-Transfer-Protocol/Server.git
cd Server
cargo build --release --offline      # all crates are vendored
./target/release/adatp-server
```

### Option B — Docker

```bash
cd deploy/docker
docker compose up --build -d
```

Either way the server listens on `0.0.0.0:3000`:

```bash
curl http://127.0.0.1:3000/healthz   # {"status":"ok"}
```

> **Port 3000 already taken?** (Vite and friends love it.) Run
> `PORT=3100 ./target/release/adatp-server` and substitute `3100`
> everywhere below. See [`docs/deployment/ports.md`](../deployment/ports.md).

## 2. Smoke-test with the CLI

From the Server workspace root:

```bash
cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123
```

Expected output ends with a real X25519 handshake and an encrypted login:

```
Sent HANDSHAKE_COMPLETE -> Secure session established 🔒
✅ Login OK: {"role":"user","user_id":"user1","username":"user1"}
```

## 3. First client — Node.js

```bash
cd sdks/node && npm install && npx tsc
```

```js
// hello.mjs — run with: node hello.mjs
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const { AdaTPClient } = require('./dist/client.js');

const client = new AdaTPClient('127.0.0.1', 3000);
await client.connect();                                   // WS + X25519
await client.authenticate('user1', 'password123');
await client.joinRoom('lobby');

client.setMessageHandler((sender, text) => console.log(`<${sender.slice(0, 6)}>`, text));
await client.sendTextMessage('Hello from Node!');          // you receive your own echo

setTimeout(() => client.disconnect(), 2000);
```

## 4. First client — browser

```bash
cd sdks/js && npm install && npm run build
# serve the workspace over HTTP (ES modules refuse file://)
cd ../.. && python3 -m http.server 8080
```

```html
<script type="module">
import { AdaTPChat } from './sdks/js/dist/adatp.js';

const chat = new AdaTPChat('ws://127.0.0.1:3000/ws', {
    username: 'user1',
    password: 'password123',
    onMessage: (text, senderId) => console.log(senderId.slice(0, 6), text),
});
chat.on('auth', () => {          // wait for AuthSuccess before joining
    chat.join('lobby');
    chat.say('Hello from the browser!');
});
chat.on('auth_failure', (reason) => console.error('login rejected:', reason));
</script>
```

The browser SDK speaks plaintext AdaTP (no in-protocol handshake) — use
`wss://` in production ([why](faq.md)).

## 5. First client — Python

```bash
pip install cryptography websocket-client
cd sdks/python
```

```python
# hello.py — run with: PYTHONPATH=src python3 hello.py
from adatp.client import AdaTPClient

client = AdaTPClient('127.0.0.1', 3000)
client.connect()                                  # WS + X25519
client.authenticate('user1', 'password123')
client.join_room('lobby')

client.send_text_message('Hello from Python!')
print('echo:', client.read_text_message())        # your own broadcast echo
client.disconnect()
```

Run the Node and Python clients side by side and they will see each
other's messages in `lobby`.

## Where next

- [Concepts](concepts.md) — the mental model (rooms, identity, echo)
- [Examples index](examples-index.md) — chat UIs, calls, file transfer, a game
- [Tools & plugins](tools-and-plugins.md) — call server-side tools
- [`demos/game-lobby`](../../demos/game-lobby/README.md) — two-browser tic-tac-toe
