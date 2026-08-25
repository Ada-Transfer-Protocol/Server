# AdaTP Specification — 09: Extensions

**Status:** Normative, v1.0

AdaTP grows in three sanctioned ways: **tool packets** (server-side
capabilities exposed to clients), **plugin-emitted events** (server →
outbound webhooks), and **new message types** inside the reserved ranges.
Anything else is a protocol fork.

---

## 1. Extension philosophy

- **E-1** Receivers MUST ignore parseable packets of unknown type
  ([04-packets.md §1](04-packets.md)). This is the backbone of forward
  compatibility: new types degrade to no-ops on old peers.
- **E-2** Unknown header flag bits MUST be ignored on receipt
  ([03-framing.md §1.2](03-framing.md)).
- **E-3** Extensions MUST NOT change the meaning of existing fields or
  types; changed semantics require a version bump
  ([10-versioning.md](10-versioning.md)).
- **E-4** JSON payloads MUST tolerate unknown members (ignore, don't
  reject) unless a schema says otherwise.

## 2. Tool packets (0x0070–0x0072)

Tools are named server-side operations — implemented by server plugins —
that any authenticated client can invoke. Transport contract:

### 2.1 ToolCall (0x0070), client → server

```json
{"id":"<correlation id, client-chosen, ≤64 chars>","tool":"<name>","args":{...}}
```

- `id` — REQUIRED. Client-chosen correlation token, at most 64
  characters. The server echoes it verbatim in the reply. Uniqueness per
  connection is the client's responsibility.
- `tool` — REQUIRED. The tool name as listed by the registry (tool names
  are namespaced with dots by convention, e.g. `system.list_tools`,
  `moderation.check`).
- `args` — REQUIRED (MAY be `{}`). Validated server-side against the
  tool's declared JSON-Schema.

### 2.2 ToolResult (0x0071), server → caller only

```json
{"id":"...","tool":"...","ok":true,"result":<any JSON>}
```

### 2.3 ToolError (0x0072), server → caller only

```json
{"id":"...","tool":"...","ok":false,"error":{"code":"<slug>","message":"..."}}
```

`error.code` MUST be one of:

| Slug | Meaning |
| :-- | :-- |
| `tool_not_found` | No tool with that name is registered/enabled. |
| `tool_timeout` | The tool exceeded its execution deadline. |
| `tool_rate_limited` | Caller exceeded the tool's rate limit. |
| `tool_invalid_args` | `args` failed schema validation (or the request JSON was malformed). |
| `tool_failed` | The tool ran and raised an error. |
| `tool_forbidden` | The caller's role is not permitted to invoke this tool. |

### 2.4 Rules

- **E-5** Tool packets are **never broadcast**. The reply — exactly one
  `ToolResult` or `ToolError` per `ToolCall` — goes only to the calling
  connection.
- **E-6** Replies MAY arrive out of order relative to other traffic and
  relative to other tool calls; clients MUST correlate by `id`.
- **E-7** On an encrypted session, tool packets are encrypted like all
  server↔client traffic.
- **E-8** Unauthenticated `ToolCall`s are refused like any pre-auth
  traffic ([05-state-machines.md](05-state-machines.md)).
- **E-9** Every conforming tool server MUST provide the discovery tool
  `system.list_tools` (args `{}`), returning
  `{"tools":[{"name","description","schema"}...]}` so clients can
  enumerate capabilities at runtime.

### 2.5 Text fallback

Clients without binary tool support use the `TOOL:` / `TOOLRESULT:` text
encoding of [06-signaling.md §6](06-signaling.md); servers MUST treat it
as equivalent to the binary form.

## 3. Plugin-emitted custom events (webhooks)

Server plugins MAY emit named events (e.g. `moderation.flagged`,
`file.completed`) into the server's **webhook dispatcher**, which delivers
them as signed HTTP POSTs to configured endpoints. Properties:

- Events flow **outward** (server → HTTP); they never enter the packet
  stream, so they cannot affect data-plane latency.
- Event names are dot-namespaced, plugin-prefixed by convention.
- Delivery, signing (HMAC-SHA256), retries, and the event catalog are
  specified in the platform documentation (`docs/platform/`), not in this
  protocol spec; the protocol-level guarantee is only that webhook
  processing is asynchronous to routing.

## 4. Reserved code-point ranges

| Range | Status |
| :-- | :-- |
| `0x0001–0x00FF` | Core protocol. Assigned by this specification only. Unassigned values (e.g. `0x0046–0x004F`, `0x0062–0x006F`, `0x0082–0x008F`, `0x0095–0x009F`, `0x00A2–0x00FE`) are reserved for future core revisions — implementations MUST NOT use them. |
| `0x0100–0xFEFF` | **Experimental/extension space.** Available for coordinated experiments; a future registry will govern assignment. Ship nothing permanent here without registration. |
| `0xFF00–0xFFFE` | **Private use.** Never standardized; safe for closed deployments where both ends are controlled. |
| `0xFFFF` | Sentinel ("Unknown") in reference decoders. MUST NOT appear on the wire. |

## 5. Defining a new message type (checklist)

A proposal for a new core type MUST specify:

1. Code point (from the appropriate range) and name.
2. Direction(s) and routing class (consumed / routable / reply).
3. Payload schema (binary layout or JSON schema) and size expectations.
4. State admissibility (which connection states accept it) and error
   behaviour, in [05-state-machines.md](05-state-machines.md) terms.
5. Security considerations (does it carry user data? is it rate-limited?).
6. A golden vector for [appendix-test-vectors.md](appendix-test-vectors.md).
7. Conformance level placement ([11-conformance.md](11-conformance.md)).

Additions of new optional types are backward-compatible (E-1) and do not
bump the protocol version; semantic changes to existing types do
([10-versioning.md](10-versioning.md)).
