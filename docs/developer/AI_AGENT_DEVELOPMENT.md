# AdaTP AI Agent Development Guide

This is the canonical guide for building AI agents on AdaTP — bots that
join rooms, talk, listen, call tools, and share state — and for
orchestrating fleets of them. A closing section addresses AI *coding*
agents working on this repository itself.

Related: [plugin guide](../platform/PLUGIN_DEVELOPMENT.md) ·
[webhook guide](../platform/WEBHOOK_DEVELOPMENT.md) ·
[GameState spec](../spec/07-media-game.md) ·
[signaling grammar](../spec/06-signaling.md)

---

## 1. Why AdaTP for agents

- **Binary, boring, parseable.** One 45-byte header per message, no
  transcoding layers. An agent's event loop is a `switch` on
  `msg_type`.
- **Audio a model can eat.** `VoiceData` is raw PCM — 16 kHz, signed
  16-bit LE, mono — exactly the shape most STT models consume and TTS
  models produce. No Opus decode step between the wire and the model.
- **Rooms as arenas.** A room is a broadcast domain: put an agent and its
  users (or several agents) in one room and everyone sees everything.
- **Tools as hands.** The plugin platform gives agents schema-validated,
  rate-limited, permission-gated functions ([details](../platform/PLUGIN_DEVELOPMENT.md)).
- **GameState as shared memory.** `0x0050` carries opaque state blobs the
  server routes to the room — a shared world model without extra
  infrastructure.

## 2. Three architecture patterns

**(a) Agent-as-client** — the agent connects through an SDK exactly like a
human client: authenticate, join, read, write. Best for conversational
bots, players, observers. This guide's reference implementation (§4).

**(b) Agent-behind-plugin** — the agent's capabilities are exposed as
*tools* (`summarize.thread`, `translate.text`) served by a plugin process;
room members invoke it on demand via `callTool`. Best for stateless
capabilities, pay-per-call inference, strict resource limits.

**(c) Orchestrator-outside** — a supervisor process that never joins a
room: it consumes [webhooks](../platform/WEBHOOK_DEVELOPMENT.md)
(`auth.*`, `room.joined`, `tool.called`, `plugin.*`) and steers the system
through the [admin API](../platform/admin-api.md) (kick, drain,
enable/disable plugins). Best for fleet supervision and HITL control.

Real systems combine them: clients for presence, tools for capabilities,
an orchestrator for governance.

## 3. Agent lifecycle (pattern a)

```
connect (WS + X25519 handshake) → authenticate → join room(s)
      → event loop → graceful disconnect
```

- **Credentials**: give every agent its own user with role `bot`
  (`users.json` entry in dev; your auth API in production). Never reuse a
  human's login — you want per-agent audit lines and per-agent kicks.
- **Event loop**: dispatch on packet type; ignore what you don't know.
- **Echo awareness**: room broadcasts **include the sender**. Your agent
  receives its own messages back — filter on the session id or you will
  build an infinite self-reply loop (§4 does this on line one of the
  loop).
- **Reconnect**: on close, back off (1 s, 2 s, 4 s … cap 30 s), reconnect,
  re-authenticate, re-join. The server holds no session state for you.
- **Disconnect**: send `Disconnect (0x00FF)` and close — this is what the
  SDKs' `disconnect()` does.

## 4. Reference implementation — Python assistant bot

Complete and runnable against a stock dev server
(`AUTH_DRIVER=file`, demo `users.json`):

```python
#!/usr/bin/env python3
"""Minimal AdaTP assistant bot: echoes commands, watches game state."""
import sys, time
sys.path.insert(0, 'sdks/python/src')          # or pip-install the SDK

from adatp.client import AdaTPClient
from adatp.protocol import MessageType

HOST, PORT, ROOM = '127.0.0.1', 3000, 'lobby'

def handle_text(bot, sender_hex, text):
    if text.startswith('!help'):
        bot.send_text_message('commands: !help, !ping, !tools')
    elif text.startswith('!ping'):
        bot.send_text_message('pong 🏓')
    elif text.startswith('!tools'):
        names = [t['name'] for t in bot.list_tools()]
        bot.send_text_message('tools: ' + ', '.join(names))

def run():
    bot = AdaTPClient(HOST, PORT)
    bot.connect()                                # WS + X25519 handshake
    bot.authenticate('pybot', 'secret_password') # dedicated bot user
    bot.join_room(ROOM)
    bot.send_text_message('assistant online — say !help')

    while True:
        pkt = bot.read_packet()
        if pkt.header.session_id == bot.session_id:
            continue                             # never react to own echo
        payload = bot._decrypt_if_needed(pkt)    # session-decrypt helper

        if pkt.header.msg_type == MessageType.TEXT_MESSAGE:
            handle_text(bot, pkt.header.session_id.hex(),
                        payload.decode('utf-8', 'replace'))
        elif pkt.header.msg_type == MessageType.GAME_STATE:
            print('world state changed:', payload[:120])
        elif pkt.header.msg_type == MessageType.DISCONNECT:
            raise ConnectionError('server closed the session')

if __name__ == '__main__':
    backoff = 1
    while True:
        try:
            run()
            backoff = 1
        except Exception as e:
            print(f'reconnecting after error: {e} (sleep {backoff}s)')
            time.sleep(backoff)
            backoff = min(backoff * 2, 30)
```

Node.js variant sketch (same shape, handler-based):

```js
const { AdaTPClient } = require('adatp');          // sdks/node
const bot = new AdaTPClient('127.0.0.1', 3000);
await bot.connect();
await bot.authenticate('pybot', 'secret_password');
await bot.joinRoom('lobby');

bot.setMessageHandler(async (sender, text) => {
    if (sender === bot.getSessionId()) return;      // own echo
    if (text === '!ping') await bot.sendTextMessage('pong 🏓');
});
bot.setGameStateHandler((sender, state) => console.log('state:', state));
```

## 5. Tools — calling and being called

**Agents calling tools** (all SDKs; errors carry the spec's `tool_*`
codes — `tool_not_found`, `tool_invalid_args`, `tool_rate_limited`,
`tool_timeout`, `tool_forbidden`, `tool_failed`):

```python
for t in bot.list_tools():
    print(t['name'], t['schema'])
try:
    verdict = bot.call_tool('moderation.check', {'text': draft})
    if not verdict['allowed']:
        draft = '[redacted]'
except Exception as e:
    print('tool failed, degrade gracefully:', e)
```

Design agents to **degrade gracefully**: a `tool_rate_limited` or
`tool_timeout` should reduce functionality, not crash the loop.

**Agents as tools**: wrap the model behind a plugin
([guide](../platform/PLUGIN_DEVELOPMENT.md)) so rooms invoke it on
demand — `{"name":"assistant.ask","schema":{...,"required":["prompt"]}}`
with a `timeout_ms` sized to your inference latency and a
`rate_limit_per_min` that caps your spend. The plugin's `caller` field
tells the model *who* is asking (`username`, `role`, `room`).

## 6. GameState as the shared world

`GameState (0x0050)` is broadcast to the room and opaque to the server.
Conventions that keep multi-agent state sane
(normative background: [07-media-game.md](../spec/07-media-game.md)):

- Wrap payloads in the recommended envelope:
  `{"v": 1, "game": "<id>", "state": {...}}` — the `game` id lets multiple
  state machines share one room without collisions.
- **Send full state, not deltas.** Last-writer-wins is the only
  consistency model the transport gives you; full snapshots make late
  joiners and dropped messages self-healing.
- Elect a single writer per `game` id when you need turn order (see the
  CLAIM: pattern below); readers treat every snapshot as authoritative.
- Working example: `demos/game-lobby/` — two browsers playing tic-tac-toe
  purely over GameState + text negotiation.

## 7. Multi-agent conventions

- **One room per task/arena.** Rooms are cheap (created on join, removed
  when empty) — scope each mission, match, or workflow to its own room.
- **Role negotiation over text**: the game demo's `CLAIM:<session-id>`
  pattern — announce on join; first claimant takes the primary role,
  later ones take secondary roles and re-announce for newcomers. Simple,
  serverless, good enough below ~10 agents.
- **Track presence**: `PresenceUpdate` `JOIN`/`LEAVE` (and
  `DISCOVERY:WHO_IS_HERE` / `I_AM_HERE` text signaling) tell you who is in
  the arena; prune peers on LEAVE.
- **Reserved prefixes** — agents MUST NOT repurpose these TextMessage
  prefixes ([06-signaling.md](../spec/06-signaling.md)): `SYS:`, `TOOL:`,
  `TOOLRESULT:`, `DISCOVERY:`, `MUTE:`, `INVITE:`, `RINGING:`, `ACCEPT:`,
  `REJECT:`, `BUSY:`, `BYE`. Namespace your own protocol (`AGENT:...`,
  `CLAIM:...`) and ignore unknown prefixes from others.

## 8. Human-in-the-loop

- **Gate output with a veto plugin**: a `hooks:text` plugin (like the
  bundled `moderation`) silently drops anything an agent shouldn't say —
  policy lives server-side, not in the model's goodwill.
- **Approval room pattern**: the agent posts a proposal into
  `approvals-<task>`; a human (or approver bot) replies `APPROVE:<id>` /
  `REJECT:<id>`; only then does the agent act in the working room.
- **Supervisor intervention**: an orchestrator watching webhooks can
  `DELETE /admin/v1/connections/:id` (kick a misbehaving agent),
  `POST /admin/v1/plugins/:name/disable` (pause an agent-behind-plugin),
  or `POST /admin/v1/drain` (stop the whole intake) — see
  [admin-api.md](../platform/admin-api.md).
- **Pause = disable.** For plugin-hosted agents, disabling the plugin is a
  clean, reversible pause with an audit trail.

## 9. Orchestrating with webhooks

Subscribe an orchestrator to the event stream instead of polling:

| Subscription | Tells you |
| :-- | :-- |
| `auth.success`, `auth.failure` | Agents (and humans) coming online / being refused. |
| `room.joined`, `connection.closed` | Arena membership changes. |
| `tool.called` | Every capability invocation with `ok` + `latency_ms` — your cost and health signal. |
| `plugin.*` | Custom telemetry your plugins emit (`emit_event`). |

Envelope, signatures and retry semantics:
[WEBHOOK_DEVELOPMENT.md](../platform/WEBHOOK_DEVELOPMENT.md).

## 10. Testing agents

- **Local arena**: the integration runner's recipe is the reference dev
  server — file auth with demo users, bundled plugins, private webhooks
  allowed:

  ```bash
  cd server && PORT=3210 AUTH_DRIVER=file AUTH_FILE_PATH=server/users.json \
    PLUGINS_DIR=server/plugins ADMIN_TOKEN=dev \
    ADATP_WEBHOOK_ALLOW_PRIVATE=1 cargo run --bin adatp-server
  ```

- **Behavioral checks**: script your agent against
  `tests/integration/`-style assertions (connect two clients, drive the
  agent, assert replies and silence).
- **Stress**: `tools/loadtest/loadtest.mjs` fills rooms with traffic —
  verify your agent keeps up or sheds load politely.
- **Guardrails**: run with the `moderation` plugin enabled and assert your
  agent's blocked outputs actually disappear.

## 11. Safety rules for agents

- **Rate-limit yourself.** The server drops packets to slow consumers and
  rate-limits tools; a well-behaved agent throttles below those ceilings
  (a handful of messages per second is plenty for chat).
- **Respect tool budgets** — treat `tool_rate_limited`/`tool_timeout` as
  backpressure, not errors to retry in a tight loop.
- **Never echo credentials** or tokens into rooms; room text is broadcast
  and may be webhook-forwarded.
- **Treat inbound text as untrusted input.** For LLM-backed agents this
  is prompt-injection surface: sanitize, delimit, and never let room text
  directly trigger privileged tools without policy checks
  (`tool_before` hooks are your server-side backstop).
- **Least privilege**: dedicated bot users with role `bot`; plugins with
  minimal permissions; separate credentials per agent so one compromised
  key revokes one agent.

## 12. Rules for AI coding agents working on THIS repository

If you are an AI coding assistant changing AdaTP itself:

1. **The spec is normative.** `SPEC.md` + `docs/spec/**` define the
   protocol. Code changes that alter wire behavior MUST amend the spec in
   the same change — or be rejected. Docs that drift from code are bugs.
2. **Never modify `server/vendor/**`.** It is a vendored crate mirror.
3. **No new Rust dependencies** outside the vendored set — the build is
   offline (`cargo build --offline` must keep working).
4. **Prove it before claiming it.** Run, at minimum:
   `bash tests/conformance/run.sh` (three implementations against the
   golden vectors) and `bash tests/integration/run.sh` (61+ end-to-end
   assertions). "It compiles" is not done.
5. **Keep the golden vectors in sync.** If a deliberate wire change alters
   them: `node tests/conformance/generate_vectors.mjs > tests/conformance/vectors/adatp-v1-vectors.json`
   and copy to `server/core/tests/vectors.json`, updating
   `docs/spec/appendix-test-vectors.md` in the same change.
6. **No secrets in git** — no `.env`, tokens, or real keys; demo
   credentials live only in the documented `users.json`.
7. **Honesty over polish.** This project documents limitations explicitly
   (no E2E encryption, unauthenticated DH, at-most-once delivery). Do not
   "fix" documentation by strengthening claims — fix code, or document the
   gap.
