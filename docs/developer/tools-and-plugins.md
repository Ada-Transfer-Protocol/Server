# Tools & Plugins — the client side

Server-side **plugins** expose **tools**: named, schema-described
functions your client can call over the same connection it chats on.
This page is the caller's story; to *build* plugins, go to
[PLUGIN_DEVELOPMENT.md](../platform/PLUGIN_DEVELOPMENT.md).

## Discovering tools

The built-in `system.list_tools` tool enumerates everything callable:

```js
const tools = await client.listTools();
/* [ { name: 'system.list_tools', description: …, schema: {…}, plugin: 'server' },
     { name: 'echo.say',          description: …, schema: {…}, plugin: 'echo' },
     { name: 'moderation.check',  description: …, schema: {…}, plugin: 'moderation' } ] */
```

Each entry carries a JSON-Schema-subset `schema` describing `args`
(`type`, `properties`, `required`, `enum`, `items`) — enough to build a
form or validate before calling.

## Calling

```js
const result = await client.callTool('echo.say', { text: 'hi', uppercase: true });
```
```python
result = client.call_tool('echo.say', {'text': 'hi', 'uppercase': True})
```
```php
$result = $client->callTool('echo.say', ['text' => 'hi', 'uppercase' => true]);
```

On the wire this is `ToolCall (0x0070)` with a JSON body:

```json
{ "id": "<correlation id, ≤64 chars, you choose>", "tool": "echo.say", "args": { … } }
```

The server answers **only you** (never the room) with `ToolResult
(0x0071)` or `ToolError (0x0072)`:

```json
{ "id": "…", "tool": "echo.say", "ok": true,  "result": { "echoed": "HI", "caller": "user1" } }
{ "id": "…", "tool": "nope",     "ok": false, "error": { "code": "tool_not_found", "message": "…" } }
```

Correlation by `id` means calls overlap freely with chat traffic and with
each other — the SDKs route replies to the right caller automatically.

## Error codes

| Code | Meaning | Retry? |
| :-- | :-- | :-- |
| `tool_not_found` | no such tool (unloaded plugin?) | after re-listing |
| `tool_invalid_args` | args failed schema/size validation (≤64 KiB) | fix args |
| `tool_rate_limited` | per-tool per-minute budget exhausted | after backoff |
| `tool_timeout` | plugin didn't answer within its `timeout_ms` | maybe — the call may still have executed |
| `tool_failed` | plugin crashed / malformed reply / internal error | maybe |
| `tool_forbidden` | a policy hook vetoed the call | no |

Node throws an `Error` with `.code`; Python/PHP raise with the code in
the message. Treat `tool_timeout` as *unknown outcome*, not failure —
design tools idempotent where it matters.

## The text fallback

Clients too small for the tool packets (or debugging by hand) can use
`TextMessage`:

- **Request:** payload `TOOL:` + the exact ToolCall JSON.
- **Reply:** a TextMessage back to you only, `TOOLRESULT:` + the
  ToolResult/ToolError JSON.

```
> TOOL:{"id":"t1","tool":"echo.say","args":{"text":"hi"}}
< TOOLRESULT:{"id":"t1","tool":"echo.say","ok":true,"result":{"echoed":"hi","caller":"user1"}}
```

Fallback requests are intercepted before room routing — they are never
broadcast.

## Ground rules for callers

- **Authenticate first** — tool calls before `AuthSuccess` get
  `AuthFailure {"error":"not_authenticated"}`.
- Respect declared `timeout_ms`/rate limits from the tool listing; the
  Node SDK's own `callTool` timeout defaults to 15 s.
- Args are validated server-side against the schema subset; extra keys
  the schema doesn't mention pass through to the plugin.
- Everything a tool learns about you is the caller context
  `{username, role, session, room}` — plugins may authorize on it.

## Observability

Every call updates per-plugin metrics (calls, errors, latency) visible in
the [Silo Panel](../platform/silo-panel.md) → PLUGINS and via
[`GET /admin/v1/plugins`](../platform/admin-api.md); each call also emits
a `tool.called` [webhook event](webhooks-for-apps.md).
