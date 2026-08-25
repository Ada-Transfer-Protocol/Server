# AdaTP Developer Portal

AdaTP (Ada Transfer Protocol) is a binary realtime protocol over WebSocket:
one 45-byte-header packet per binary message, rooms for routing, optional
X25519/AES-256-GCM session encryption, first-class file transfer, raw PCM
voice, shared game state, and a plugin tool platform. The server listens on
**port 3000**, endpoint **`/ws`**.

Start here:

| Page | What it covers |
| :-- | :-- |
| [Getting started](getting-started.md) | Server up + first client in Node, browser JS, Python — in ~10 minutes |
| [Concepts](concepts.md) | Packets, rooms, identity, plaintext vs secure sessions |
| [Protocol guide](protocol-guide.md) | The wire format from a developer's seat |
| [SDK: Browser JS](sdk-js.md) | `AdaTPChat`, `AdaTPGame`, `AdaTPConference`, `AdaTPPhone`, file transfer |
| [SDK: Node.js](sdk-node.md) | `AdaTPClient` with encryption, tools, game state |
| [SDK: Python](sdk-python.md) | Sync client with encryption, tools, game state |
| [SDK: C](sdk-c.md) | C11 client for native/embedded hosts |
| [SDK: PHP](sdk-php.md) | Streams-based client + Laravel integration |
| [Voice](voice.md) | PCM audio, conferences, 1:1 calls, signaling |
| [File transfer](file-transfer.md) | Init/chunk/complete flow and receiver patterns |
| [Game state](game-state.md) | `GameState (0x0050)` shared-state packets |
| [Tools & plugins](tools-and-plugins.md) | Calling server-side tools from clients |
| [Webhooks for apps](webhooks-for-apps.md) | Reacting to server events in your backend |
| [Error handling](error-handling.md) | Failure codes, close reasons, retry guidance |
| [Versioning](versioning.md) | The version byte and compatibility promises |
| [Examples index](examples-index.md) | Every runnable example with its run command |
| [FAQ](faq.md) | Port conflicts, encryption scope, persistence… |
| [Glossary](glossary.md) | The project's vocabulary |
| [Building from source](building-from-source.md) | Server + all SDKs |

Going deeper:

- **Platform guides** — building [plugins](../platform/PLUGIN_DEVELOPMENT.md),
  [webhooks](../platform/WEBHOOK_DEVELOPMENT.md),
  [AI agents](AI_AGENT_DEVELOPMENT.md), the
  [admin API](../platform/admin-api.md) and the
  [Silo Panel](../platform/silo-panel.md).
- **Normative specification** — [`docs/spec/`](../spec/00-overview.md),
  indexed from the repository root [`SPEC.md`](../SPEC.md).
- **Operations** — the [production portal](../production/README.md).
