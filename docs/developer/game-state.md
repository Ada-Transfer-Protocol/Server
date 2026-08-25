# Game State

`GameState (0x0050)` is a first-class packet for **room-scoped shared
state**: boards, lobbies, cursors, world snapshots, agent blackboards.
The server treats the payload as opaque bytes and room-broadcasts it like
text — the semantics live entirely in your clients.

## The recommended envelope

Payloads are yours, but the convention every AdaTP example follows is:

```json
{ "v": 1, "game": "tictactoe", "state": { "board": [""," ",…], "turn": "X" } }
```

- `v` — your state-schema version.
- `game` — a discriminator so unrelated apps sharing a room ignore each
  other.
- `state` — the **complete** current state.

## Consistency model: last-writer-wins snapshots

AdaTP gives you ordered fan-out within one sender, no ordering across
senders, at-most-once delivery, and no persistence. The pattern that works
under those rules:

1. **Send full snapshots, not deltas.** Any received state fully replaces
   the local one — a dropped packet costs you one frame of staleness, not
   corruption.
2. **One writer at a time.** Turn-based games get this free (only the
   player to move writes). Real-time apps should partition state per
   writer or elect a host.
3. **Late joiners need a re-send.** Nothing is stored server-side; an
   existing member re-broadcasts current state when someone arrives
   (presence `JOIN` or an application-level hello).

The [game lobby demo](../../demos/game-lobby/README.md) shows all three:
players negotiate X/O by broadcasting `CLAIM:<sessionId>` text messages
(first claim wins, the second player concedes to O), every move sends the
whole board, and the X player re-announces + re-broadcasts state when a
newcomer's claim appears.

## SDK APIs

| SDK | Send | Receive |
| :-- | :-- | :-- |
| Browser JS | `AdaTPGame.sendState(obj)` | `game.on('state', (state, senderId) => …)` |
| Node | `client.sendGameState(obj)` | `setGameStateHandler(cb)` / `readNextGameState()` |
| Python | `client.send_game_state(dict)` | `client.read_game_state()` |
| PHP | `$client->sendGameState($arr)` | `$client->readGameState()` |
| C | `adatp_client_send(c, ADATP_MSG_GAME_STATE, json, len)` | `read_packet` + type check |
| Arduino | `adatp.sendGameState(json)` | `loop()` + your handler |

Objects/dicts are JSON-encoded by the SDKs; raw bytes pass through if you
have a binary format. Encrypted sessions encrypt game state like
everything else.

## GameState vs TextMessage

Use **GameState** for machine-consumed state that replaces previous state;
use **TextMessage** for human chat and the reserved signaling grammar.
Keeping them on separate types means chat UIs never parse your board and
your game loop never regexes chat. (Pre-1.0 clients shipped state through
text; the dedicated type is the v1 way.)

## Sizing

A state snapshot must fit one packet (`MAX_FRAME_BYTES`, default 1 MiB —
plenty for JSON boards). High-frequency updates: budget
`rate × size × members` per room; the [load test](../testing/README.md)
gives you the measurement harness. For 60 Hz-ish needs, send at 10–20 Hz
and interpolate client-side.

## Worked example

Two browser windows, full flow — `demos/game-lobby/`:

```bash
cd server && cargo run --bin adatp-server        # port 3000 (or PORT=…)
cd sdks/js && npm install && npm run build
python3 -m http.server 8080                      # from the workspace root
open http://localhost:8080/demos/game-lobby/
```

Log in as `user1`/`password123` and `admin`/`secret_password`, join the
same match room, play. Everything on the wire is visible in the Silo
Panel's [live view](../platform/silo-panel.md).
