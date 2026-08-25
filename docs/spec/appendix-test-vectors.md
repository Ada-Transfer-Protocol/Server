# AdaTP Specification — Appendix: Golden Test Vectors

**Status:** Normative, v1.0

The machine-readable source of truth is
[`tests/conformance/vectors/adatp-v1-vectors.json`](../../tests/conformance/vectors/adatp-v1-vectors.json)
(generated deterministically by `tests/conformance/generate_vectors.mjs`).
This appendix reproduces every case for human readers. If this page and
the JSON ever disagree, **the JSON wins** — file a bug.

All vectors share these fixed inputs unless stated otherwise:

| Constant | Value |
| :-- | :-- |
| `session_id` | `000102030405060708090a0b0c0d0e0f` |
| `timestamp` | `1700000000000` ms (`0x018bcfe56800`) |
| `shared_secret` | `000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f` |
| HKDF salt | 32 zero bytes |

Conformance requirements for replaying these vectors are in
[11-conformance.md §2.1](11-conformance.md).

---

## 1. `frame-plaintext-text` — Plaintext TextMessage (0x0020) framing

Input: `msg_type 0x0020`, `flags 0`, `sequence 0`, payload UTF-8
`Hello, AdaTP!`.

Expected frame (58 bytes):

```
54414441 01 0000 0d000000 0000000000000000 2000 0068e5cf8b010000
000102030405060708090a0b0c0d0e0f
48656c6c6f2c20416461545021
```

Contiguous hex:

```
544144410100000d000000000000000000000020000068e5cf8b010000000102030405060708090a0b0c0d0e0f48656c6c6f2c20416461545021
```

## 2. `frame-plaintext-joinroom` — Plaintext JoinRoom (0x00A0) framing

Input: `msg_type 0x00A0`, `flags 0`, `sequence 0`, payload UTF-8 `lobby`.

Expected frame (50 bytes):

```
54414441010000050000000000000000000000a0000068e5cf8b010000000102030405060708090a0b0c0d0e0f6c6f626279
```

## 3. `kdf-hkdf-sha256` — Session key derivation

Input: the shared secret and salt above.

Expected outputs:

| Derived value | Hex |
| :-- | :-- |
| `client_write_key` (info `client_write`, 32 B) | `301399aa1f12eae58fca5d5cf30086846fda62c2fcf190ce02613a5bddcc41ee` |
| `server_write_key` (info `server_write`, 32 B) | `3a5e213e39fff1dbf96170968c89eeaa0d031915ba483f2636bbdb50490327c7` |
| `client_iv_root` (info `client_iv`, 12 B) | `2a2803c4101accc98c471b19` |
| `server_iv_root` (info `server_iv`, 12 B) | `85505ca20fb29f10b663ec88` |

## 4. `nonce-seq-xor` — Nonce derivation

Input: `iv_root = 2a2803c4101accc98c471b19`, `sequence = 5`.

Expected nonce: `2a2803c4151accc98c471b19`
(only byte 4 changes: `0x10 ⊕ 0x05 = 0x15`; the remaining seven XOR bytes
of `le64(5)` are zero).

## 5. `frame-encrypted-text-client` — Encrypted TextMessage from client

Input: plaintext UTF-8 `secret message`, `client_write` key from case 3,
`sequence 1`, `flags 0x0001`, no AAD.

Expected:

| Field | Hex |
| :-- | :-- |
| nonce | `2a2803c4111accc98c471b19` |
| ciphertext (14 B) | `63ed1e55e08c367187732cefd1cf` |
| auth tag | `74772e5928b8238c2d333bbdd2546b64` |

Expected full frame (75 bytes):

```
544144410101000e000000010000000000000020000068e5cf8b010000000102030405060708090a0b0c0d0e0f63ed1e55e08c367187732cefd1cf74772e5928b8238c2d333bbdd2546b64
```

## 6. `frame-encrypted-gamestate-server` — Encrypted GameState from server

Input: plaintext UTF-8 `{"board":[1,0,2],"turn":"p1"}` (29 bytes),
`server_write` key from case 3, `sequence 2`, `flags 0x0001`,
`msg_type 0x0050`.

Expected:

| Field | Hex |
| :-- | :-- |
| nonce | `85505ca20db29f10b663ec88` |
| ciphertext (29 B) | `1f32f30320d62920f2838e2a5a0a0b677a950f537195d7dba7d2816bf1` |
| auth tag | `efbffb990d557a890faacabeffa17d28` |

Expected full frame (90 bytes):

```
544144410101001d000000020000000000000050000068e5cf8b010000000102030405060708090a0b0c0d0e0f1f32f30320d62920f2838e2a5a0a0b677a950f537195d7dba7d2816bf1efbffb990d557a890faacabeffa17d28
```

## 7. `reject-bad-magic` — wrong magic MUST be rejected

Input frame (valid layout, magic replaced with `deadbeef`):

```
deadbeef01000001000000000000000000000020000068e5cf8b010000000102030405060708090a0b0c0d0e0f78
```

Expected: decode error `invalid_magic` (any refusal surface passes; a
successful decode fails the case).

## 8. `reject-short-header` — truncated header MUST be rejected

Input (30 bytes — a 45-byte header cut short):

```
5441444101000000000000000000000000000020000068e5cf8b01000000
```

Expected: decode error `short_header`.

## 9. `reject-tampered-tag` — tampered ciphertext MUST fail decryption

Input: take the full frame of case 5 (`frame-encrypted-text-client`) and
XOR its **last byte** with `0x01` (turning the tag's final byte `64` into
`65`).

Expected: AES-GCM authentication failure (`decrypt_failed`). An
implementation that returns plaintext fails the case.
