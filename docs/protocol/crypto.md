# AdaTP Cryptography — What It Protects, and What It Doesn't

**Audience:** developers and reviewers. Non-normative narrative; the
normative text is [docs/spec/08-security.md](../spec/08-security.md).

---

## 1. The honest summary first

AdaTP's built-in encryption is **client↔server transport security** —
think "a lightweight TLS-shaped channel inside the protocol":

| Property | AdaTP secure session |
| :-- | :-- |
| Passive eavesdropper on the network reads traffic | **No** (AES-256-GCM) |
| Tampered packets accepted | **No** (GCM tag; connection closes) |
| Active man-in-the-middle defeated | **No** — the X25519 exchange is unauthenticated. TLS (`wss://`) is the MITM defense. |
| Server can read your traffic | **Yes.** The server decrypts to route. This is *not* end-to-end encryption. |
| Forward secrecy | Yes per connection (ephemeral keys, never stored) |

Production rule: **always `wss://`** (TLS terminated at your proxy or
Cloudflare), with the AdaTP layer as defense in depth. Plain `ws://`
belongs on localhost only.

## 2. Why these primitives

- **X25519** — small (32-byte keys), constant-time, universally available
  (libsodium, OpenSSL, WebCrypto-adjacent libs, mbedTLS on ESP32).
- **HKDF-SHA256** — the standard extract-then-expand KDF (RFC 5869);
  trivially portable, even hand-buildable from HMAC on microcontrollers.
- **AES-256-GCM** — AEAD with hardware acceleration on x86, ARM, and the
  ESP32's crypto engine; one primitive gives confidentiality + integrity.

Nothing exotic: every SDK reuses its platform's audited implementation
(Node `crypto`, Python `cryptography`, PHP OpenSSL+sodium, C OpenSSL,
ESP32 mbedTLS, Rust `x25519-dalek`/`aes-gcm`/`hkdf`).

## 3. Key derivation, step by step

Inputs after the handshake: the 32-byte X25519 shared secret.

```
PRK  = HMAC-SHA256(key = 32 zero bytes, msg = shared_secret)   # HKDF-Extract
okm(info, L): T1 = HMAC-SHA256(PRK, info || 0x01); return first L bytes
client_write_key = okm("client_write", 32)
server_write_key = okm("server_write", 32)
client_iv_root   = okm("client_iv", 12)
server_iv_root   = okm("server_iv", 12)
```

Each direction gets its own key and IV root: the client encrypts with
`client_write_*`, the server with `server_write_*`, so the two packet
streams never share nonce space.

**Check yourself against the golden vector** (shared secret
`000102…1e1f`, from
[appendix-test-vectors.md](../spec/appendix-test-vectors.md)):

```
client_write_key = 301399aa1f12eae58fca5d5cf30086846fda62c2fcf190ce02613a5bddcc41ee
server_write_key = 3a5e213e39fff1dbf96170968c89eeaa0d031915ba483f2636bbdb50490327c7
client_iv_root   = 2a2803c4101accc98c471b19
server_iv_root   = 85505ca20fb29f10b663ec88
```

## 4. Per-packet encryption

Each encrypted packet:

1. Sender increments its direction's sequence counter (starts at 1).
2. Nonce = IV root with the last 8 bytes XORed by the little-endian
   sequence — unique per packet because sequences never repeat.
3. AES-256-GCM over the plaintext payload, empty AAD.
4. Frame carries: `flags.ENCRYPTED`, the sequence, the ciphertext
   (`length` = ciphertext size), and the 16-byte tag after the payload.

Decryption failure is terminal — the connection closes
(`decrypt_failed`). There is no "skip the bad packet" mode; a broken tag
means a broken or hostile channel.

### Why the sequence lives in the header

The receiver needs the sender's sequence to rebuild the nonce. Carrying it
(rather than counting locally) makes decryption stateless per packet and
keeps a lost packet from desynchronising the stream — at the price of
best-effort-only replay protection
([08-security.md §3.5](../spec/08-security.md)).

## 5. The mixed-room trick (and its consequence)

Rooms can contain plaintext browsers and encrypted device clients
simultaneously. The server makes this work by decrypting each inbound
packet with the **sender's** session and re-encrypting outbound copies
with **each recipient's** session.

The consequence is the headline limitation: **the server sees plaintext**.
That is an architectural property, not a bug — routing, moderation
plugins, and tools all rely on it. If your threat model includes the
server, AdaTP's layer is not your confidentiality boundary; you need
application-level E2E on top (encrypt payloads client-side under keys the
server never sees — the protocol happily carries opaque bytes).

## 6. SDK interop notes

All six SDKs interoperate because they agree on §3–§4 bit-for-bit:

| SDK | X25519 | HKDF | AES-GCM | Notes |
| :-- | :-- | :-- | :-- | :-- |
| Rust (server/core) | x25519-dalek | hkdf crate | aes-gcm crate | `SessionKeys::derive(secret, salt)` — callers pass the 32-zero-byte salt. |
| Node.js | `crypto` DH (SPKI wrap) | `hkdfSync` | `createCipheriv` | Raw key = last 32 bytes of the SPKI DER export. |
| Python | `cryptography` | `HKDF` | `AESGCM` | Straightforward. |
| PHP | libsodium `crypto_scalarmult` | `hash_hkdf` | OpenSSL `aes-256-gcm` | sodium "box" keys are plain X25519 keys — compatible. |
| C | OpenSSL EVP X25519 | EVP HKDF | EVP GCM | Sets salt + info per derivation. |
| Arduino/ESP32 | mbedTLS ECDH on Curve25519 (with LE↔BE `reverseBytes` on MPI export/import) | **hand-rolled HKDF from HMAC-SHA256** | mbedTLS GCM | The hand-rolled HKDF is the RFC 5869 single-block case: `Extract = HMAC(salt, ikm)`, `T1 = HMAC(PRK, info‖0x01)`, truncate — identical output for L ≤ 32, which covers all four derivations. |

Cross-checking tip: the `kdf-hkdf-sha256` vector catches every classic
mistake here (swapped salt/IKM, missing `0x01`, big-endian sequence XOR).

## 7. Credential handling

- `AuthRequest` carries `{"username","password"}` JSON — encrypted when a
  secure session exists; otherwise it relies on TLS. Deploy so that
  "neither" cannot happen.
- Server-side verification is fail-closed and constant-time for the file
  driver; the file driver itself (plaintext `users.json`) is a
  development convenience only. Production: `AUTH_DRIVER=api` against
  your identity service, which receives the password over your own HTTPS
  endpoint and returns `{"authorized", "user_id", "role"}`.
- The AdaTP server never stores passwords; the SQLite database holds only
  control-plane API keys.

## 8. Known gaps, stated plainly

1. No E2E (§5) — by design; carry pre-encrypted payloads if you need it.
2. Unauthenticated handshake — TLS is mandatory against active attackers.
   (`adatp-core` ships Ed25519 signing primitives; binding server identity
   into the handshake is future work, tracked for a later wire version.)
3. Best-effort replay resistance — no strict sliding window in v1.
4. No key rotation within a connection — reconnect to re-key.
5. Metadata (types, sizes, timing, session ids) is visible to any observer
   of a non-TLS link even when payloads are encrypted.

If any documentation elsewhere claims more than this page, this page and
[08-security.md](../spec/08-security.md) win — please file a bug.
