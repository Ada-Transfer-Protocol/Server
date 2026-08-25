# AdaTP Game Lobby demo — Tic-Tac-Toe

Two browser windows play tic-tac-toe through an AdaTP room. Every move is a
`GameState (0x0050)` packet broadcast to the room; player symbols are
negotiated with a tiny `CLAIM:` text-message handshake.

## Run

```bash
# 1. Start the server (any free port; 3000 is canonical)
cd server && PORT=3000 cargo run --bin adatp-server

# 2. Build the browser SDK once
cd sdks/js && npm install && npm run build

# 3. Serve the workspace root (ES modules need HTTP, not file://)
python3 -m http.server 8080          # from the workspace root

# 4. Open two windows
open http://localhost:8080/demos/game-lobby/
```

Log in with two different demo users (e.g. `user1`/`password123` and
`admin`/`secret_password` from `server/server/users.json`), join the same
match room, and play.

## What it demonstrates

- `AdaTPGame` SDK class: `join(room)`, `sendState(obj)`, `on('state', …)`
- Room-scoped GameState routing (server treats the payload as opaque JSON)
- Presence + text signaling alongside game traffic on one connection

Docs: [`docs/spec/07-media-game.md`](../../docs/spec/07-media-game.md) ·
[`docs/developer/game-state.md`](../../docs/developer/game-state.md)
