# Authentication Providers

The server verifies every `AuthRequest` against one driver
(`AUTH_DRIVER`). Three exist. Common behavior regardless of driver:
3 failed attempts close the connection; unauthenticated connections cannot
join rooms or send traffic; a broken backend **fails closed**
(`auth_unavailable` — nobody gets in by accident).

## `file` — development & demos only

```env
AUTH_DRIVER=file
AUTH_FILE_PATH=users.json
```

`users.json` format (array):

```json
[
  { "username": "alice",  "password": "plaintext-here", "role": "user"  },
  { "username": "gamebot","password": "…",              "role": "bot"   }
]
```

Facts to respect:

- Passwords are **plaintext on disk**. That is why this driver is labeled
  dev/demo-only everywhere. Do not invent a hashing scheme that isn't there.
- Comparison is constant-time; `role` defaults to `user` if omitted.
- The file is read at boot. Runtime reload without restart:

  ```bash
  curl -X POST -H "x-admin-token: $ADMIN_TOKEN" \
    http://127.0.0.1:3000/admin/v1/users/reload
  # → {"ok":true,"users":6}
  ```

- The repo's demo file (`server/server/users.json`) contains well-known
  credentials (`user1/password123`, …). It exists for tests and demos.
  **Never deploy it.**

## `api` — the production driver

```env
AUTH_DRIVER=api
AUTH_API_URL=https://auth.internal.example.com/verify
```

Contract — the server POSTs and expects JSON back:

```
POST $AUTH_API_URL          (timeout: 5 s, no retries)
{ "username": "alice", "password": "secret" }

200 → { "authorized": true,  "user_id": "u-42", "role": "admin" }
200 → { "authorized": false, "error": "bad password" }      → invalid_credentials
non-2xx                                                     → invalid_credentials
timeout / network / malformed JSON                          → auth_unavailable (connection closed)
```

`user_id` defaults to the username, `role` to `user`, when omitted.

Minimal Node/Express backend:

```js
import express from 'express';
import bcrypt from 'bcryptjs';
const app = express().use(express.json());
const users = { alice: { hash: bcrypt.hashSync('secret', 10), role: 'user', id: 'u-42' } };

app.post('/verify', (req, res) => {
    const u = users[req.body?.username];
    if (u && bcrypt.compareSync(req.body?.password ?? '', u.hash)) {
        return res.json({ authorized: true, user_id: u.id, role: u.role });
    }
    res.json({ authorized: false, error: 'invalid credentials' });
});
app.listen(8080);
```

Any stack works — a Laravel route validating against your `users` table, a
Django view over your auth model — as long as it answers the JSON contract
above. Passwords transit AdaTP→backend as JSON over the URL you configure:
make it **https on a private network**, and never log request bodies.

Operational notes:

- The 5 s timeout bounds login latency; a slow IdP slows logins, nothing else.
- Backend outage symptom: clients receive
  `AuthFailure {"error":"auth_unavailable"}` and are disconnected; existing
  sessions continue. Runbook: [incident-runbook.md](./incident-runbook.md).
- Rate-limit and audit on the backend side — it sees every attempt.

## `none` — anonymous mode

```env
AUTH_DRIVER=none
```

Every login is accepted; the username is taken as-is and the role is
`anonymous`. It exists for local experiments and open demo lobbies.
Do not run it on the internet: anyone can claim any name, join any room,
call any tool the role permits. If you must use it publicly, treat it as an
unauthenticated service in your threat model and gate everything valuable
behind plugins that check roles.

## Bot / service accounts

Give every agent, bridge, and integration its own credential with the
narrowest role (`bot`), so that:

- `auth.success` / `auth.failure` webhook events attribute activity correctly,
- Silo → CONNECTIONS shows *which* bot is misbehaving,
- revocation is one account, not a shared secret rotation.

Role strings are free-form and flow through to plugins
(`caller.role` in tool calls) — establish a small fixed vocabulary
(`user`, `admin`, `bot`, `anonymous`) and enforce it in your api driver.
