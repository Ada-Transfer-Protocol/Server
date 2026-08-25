# Browser JavaScript SDK

`sdks/js` — TypeScript, built to ES modules. One base class and five
domain clients, each owning a slice of the protocol.

```bash
cd sdks/js && npm install && npm run build     # tsup → dist/adatp.js (+ .d.ts)
```

```js
import { AdaTPChat, AdaTPGame, AdaTPConference, AdaTPPhone, AdaTpFileTransfer }
    from './sdks/js/dist/adatp.js';
```

**Transport note:** the browser SDK speaks **plaintext AdaTP** — it does
not implement the X25519 handshake (browser crypto ergonomics + the server
sits behind TLS anyway). Always use `wss://` outside localhost.

## Construction and connection

Every class shares the same constructor:

```js
const chat = new AdaTPChat('ws://127.0.0.1:3000/ws', {
    username: 'user1',
    password: 'password123',
    autoConnect: true,              // default: connects ~immediately
    onMessage: (text, senderId) => { … },
});
```

Options starting with `on` auto-bind to events (`onUserJoined` →
`user_joined`). You can also subscribe manually: `chat.on('message', cb)`.

Base API (all classes): `connect(username, password)`, `disconnect()`,
`getMyId()` (hex session id), `authenticated` (true after AuthSuccess),
`on(event, cb)`.

Base events:

| Event | Payload | When |
| :-- | :-- | :-- |
| `connect` / `disconnect` | – | socket opened / closed |
| `auth` | identity `{user_id, username, role}` | AuthSuccess |
| `auth_failure` | reason string | AuthFailure — **handle this**; the SDK keeps the socket but the server won't route for you |
| `room_joined` | room name | server confirmed JoinRoom |

Wait for `auth` before joining rooms in new code (the demo pages do).

## `AdaTPChat` — text and rooms

```js
chat.join('support');            // → 'room_joined'
chat.say('Hello!');              // you receive your own echo too
chat.on('message', (text, senderId) => …);
chat.on('user_joined', (senderId) => …);   // PresenceUpdate JOIN
chat.on('user_left', (senderId) => …);     // PresenceUpdate LEAVE
```

## `AdaTPGame` — shared state

```js
const game = new AdaTPGame(url, { username, password });
game.on('auth', () => game.join('match-1'));
game.on('state', (state, senderId) => render(state));
game.sendState({ v: 1, game: 'tictactoe', state: { board, turn } });
```

Objects are JSON-encoded into `GameState (0x0050)` packets; raw
`Uint8Array` passes through untouched. Full pattern (role negotiation,
newcomer sync): [game state](game-state.md) and
[`demos/game-lobby`](../../demos/game-lobby/README.md).

## `AdaTpFileTransfer`

```js
const ft = new AdaTpFileTransfer(url, { username, password });
ft.on('progress', pct => …);
ft.on('complete', () => …);
await ft.sendFile(fileInputElement.files[0]);   // FileInit → chunks → FileComplete
```

## `AdaTPConference` — group voice

```js
const conf = new AdaTPConference(url, { username, password,
    onUserJoined: id => …, onMuteChanged: (id, muted) => … });
conf.join('standup');        // joins room + starts mic (16 kHz PCM)
conf.toggleMute();           // broadcasts MUTE:ON / MUTE:OFF
conf.leave();                // DISCOVERY:I_AM_LEAVING + back to lobby
conf.getUsers();             // Set<sessionId> discovered via DISCOVERY:*
```

Audio is captured with WebAudio at 16 kHz mono, shipped as raw PCM
`VoiceData` packets, and played straight back on receive — no codecs.
Browsers require a user gesture before audio starts.

## `AdaTPPhone` — 1:1 calls

```js
const phone = new AdaTPPhone(url, { username, password,
    onIncomingCall: id => …, onConnected: () => …, onEnded: reason => …,
    onNetworkQuality: latencyMs => … });
phone.init();                 // joins the signaling room
phone.call(targetId);         // INVITE:<target>:<room>
phone.answer(); phone.reject(); phone.hangup(); phone.toggleMute();
phone.getCallState();         // IDLE | DIALING | INCOMING | CONNECTED
```

Signaling rides on text messages (`INVITE`/`RINGING`/`ACCEPT`/`REJECT`/
`BUSY`/`BYE`); RTT comes from timing the client's own `SYS:PING` echo.
Grammar: [voice](voice.md).

## Bundled example pages

`sdks/js/`: `chat_example.html`, `message_room_example.html`,
`message_single_example.html`, `group_call_example.html`,
`single_call_example.html`, `file_transfer.html`, `transfer_example.html`,
`test.html`. Serve the repo over HTTP and open them — see the
[examples index](examples-index.md).

There is also a slim legacy client (`dist/client.js`, `AdaTPClient`) kept
for the oldest example pages; new code should use the classes above.
