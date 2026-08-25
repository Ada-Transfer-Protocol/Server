# Examples Index

Every runnable example in the workspace, with its one-line run command.
Unless noted, start a server first (`cd server && cargo run --bin
adatp-server`, or `PORT=3100 …` if 3000 is busy) — demo logins come from
`server/server/users.json` (`user1/password123`, `admin/secret_password`,
`filebot|pybot|phpbot|cbot / secret_password`).

## Demos

| Example | Run |
| :-- | :-- |
| **Game lobby** (two-browser tic-tac-toe over GameState) — `demos/game-lobby/` | `python3 -m http.server 8080` from the workspace root → open `http://localhost:8080/demos/game-lobby/` twice |

## Browser (serve the workspace: `python3 -m http.server 8080`, then open under `http://localhost:8080/sdks/js/`)

Build once: `cd sdks/js && npm install && npm run build`

| Page | Shows |
| :-- | :-- |
| `chat_example.html` | room chat UI |
| `message_room_example.html` | room messaging |
| `message_single_example.html` | direct-style messaging |
| `group_call_example.html` | conference voice (mic permission needed) |
| `single_call_example.html` | 1:1 call with INVITE/RINGING signaling |
| `file_transfer.html` / `transfer_example.html` | file send/receive |
| `test.html` | kitchen-sink smoke page |

## Node.js (`cd sdks/node && npm install && npx tsc` first)

| Example | Run |
| :-- | :-- |
| `example.js` | `node example.js` |
| `example.ts` | `npx ts-node example.ts` |
| `filetransfer_example.ts` | `npx ts-node filetransfer_example.ts` |

## Python (`pip install cryptography websocket-client`)

| Example | Run (from `sdks/python`) |
| :-- | :-- |
| `example.py` — terminal chat with `/join`, `/quit` | `PYTHONPATH=src python3 example.py` |
| `filetransfer_example.py` — send + receive files | `PYTHONPATH=src python3 filetransfer_example.py` |

## PHP (`cd sdks/php && composer install`)

| Example | Run |
| :-- | :-- |
| `example.php` — connect, send, disconnect | `php example.php` |
| `chat-example.php` — interactive terminal chat | `php chat-example.php` |
| `filetransfer_example.php` — send + receive loop | `php filetransfer_example.php` |

## C (`cd sdks/c && cmake -B build && cmake --build build`)

| Example | Run |
| :-- | :-- |
| `examples/example.c` — terminal chat (select on stdin+socket) | `./build/adatp_example` |
| `filetransfer_example.c` — file send/receive | compile against `libadatp`, see [sdk-c.md](sdk-c.md) |

## Arduino / ESP32

| Example | Run |
| :-- | :-- |
| `sdks/arduino-esp32/examples/ESP32_Connect/ESP32_Connect.ino` | open in Arduino IDE, set WiFi + server IP (port 3000), flash an ESP32 |

## Server-side

| Example | Run |
| :-- | :-- |
| `server/plugins/echo` — reference tool plugin | loaded automatically (needs `node` on PATH); call with `client.callTool('echo.say', {text:'hi'})` |
| `server/plugins/moderation` — text-veto hook + tool | loaded automatically; say a message containing `badword` and watch it vanish |
| `tools/webhook-receiver/receiver.mjs` — signed-delivery receiver | `node tools/webhook-receiver/receiver.mjs 9099 dev-secret` (+ register, see [webhooks-for-apps](webhooks-for-apps.md)) |
| `tools/loadtest/loadtest.mjs` — load generator with latency percentiles | `cd tools/loadtest && npm install && node loadtest.mjs --url ws://127.0.0.1:3000/ws --clients 50 --duration 20` |
| `tools/adatp-cli` (in the Server repo) — protocol smoke test | `cd server && cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123` |

## Test suites as living examples

The integration suites are complete, assertion-checked client programs —
often the best reference for exact call sequences:
`tests/integration/*.mjs` and the conformance runners
`tests/conformance/run_node.mjs`, `run_python.py`
([testing guide](../testing/README.md)).
