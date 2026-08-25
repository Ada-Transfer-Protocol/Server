# 12 — Authenticated Handshake (Protocol v2)

> **Status: PROPOSED DESIGN — not yet implemented.** This chapter specifies an
> *authenticated* key exchange for AdaTP protocol **version 2**. It exists so
> the design can be reviewed and **formally verified before any code ships**
> (see [`formal/`](./formal/)). Protocol **v1 is unchanged**; nothing here is
> active in a shipped build. Do **not** describe the server as MITM-resistant on
> the basis of this document — that claim is earned only once the model checks,
> an implementation lands behind version negotiation, and the wire vectors are
> published. See [`../SECURITY_MODEL.md`](../SECURITY_MODEL.md) §11.

## 1. Why

AdaTP v1's handshake is an **unauthenticated** ephemeral X25519 exchange: the
server proves possession of no long-term identity, so an active on-path attacker
can run a separate exchange with each side (textbook MITM). v1 mitigates this
**only** by mandating TLS at the edge — TLS provides the server authentication.

That is acceptable for browser/mobile/cloud deployments (TLS is always present),
but it undercuts AdaTP's defining use case: a **constrained device** (e.g. a
~20 KB-RAM MCU) that cannot run a full TLS 1.3 stack but *can* run X25519 +
Ed25519 + AES-GCM (the C / Arduino-ESP32 SDKs already do the whole session
handshake on-device). For that peer to be secure **without** TLS, AdaTP's own
handshake must authenticate the server. Hence v2.

Design goal: the same verified, authenticated handshake on an MCU and in a
browser — no TLS required for the AdaTP security guarantees, TLS optional as an
outer layer.

## 2. Construction (SIGMA-style, server-authenticated)

The server **S** holds a long-term **Ed25519** identity key pair
(`spk_S` public, `ssk_S` secret). Clients obtain `spk_S` out of band (§4). The
exchange binds the ephemeral X25519 key agreement to `spk_S` with a signature
over the full transcript — the standard SIGMA / Noise-IK shape.

Notation: `epk_C`/`esk_C` = client ephemeral X25519 key pair; `epk_S`/`esk_S` =
server ephemeral; `||` = concatenation; `H` = SHA-256.

```
  Client C                                             Server S (spk_S, ssk_S)
  ─────────────────────────────────────────────────────────────────────────
  epk_C, esk_C ← X25519.keygen()
  ── ClientHello { ver=2, epk_C } ──────────────────────────────▶
                                        epk_S, esk_S ← X25519.keygen()
                                        transcript = LABEL || u8(2) ||
                                                     epk_C || epk_S || spk_S
                                        th  = H(transcript)
                                        sig = Ed25519.sign(ssk_S, th)
  ◀──────── ServerHello { ver=2, epk_S, spk_S, sig } ────────────

  # Client verifies BEFORE deriving anything:
  #   (1) spk_S == the key it pinned / expects for S     (else ABORT: unknown identity)
  #   (2) Ed25519.verify(spk_S, H(LABEL||u8(2)||epk_C||epk_S||spk_S), sig)  (else ABORT)

  ss  = X25519(esk_C, epk_S) = X25519(esk_S, epk_C)
  k   = HKDF-SHA256(salt, ss, info)          # identical KDF to v1
  fin = AEAD_k(header, "AdaTP-v2-finished" || th)   # encrypted, header as AAD
  ── ClientAuth { fin } ────────────────────────────────────────▶
                                        # S decrypts fin, checks the tag over th
  ◀──────── (session established; AuthRequest/… now flow encrypted) ─────────
```

- `LABEL = "AdaTP-v2-handshake"` — domain separation (distinct from any other
  signed context in the system).
- The signature covers **both ephemerals, the version, and `spk_S`** → an
  attacker who substitutes its own `epk_S'` cannot produce a matching signature
  without `ssk_S`, and cannot downgrade the version without detection.
- `ClientAuth.fin` is a key-confirmation ("Finished"): it proves C derived the
  same `k` and, because the packet header is the AEAD AAD, binds the header.
- The **X25519 → HKDF-SHA256 → AES-256-GCM** session and the nonce derivation
  are **unchanged from v1** — v2 adds identity + confirmation around the same
  primitives, so the C SDK reuses its existing crypto.

## 3. Post-handshake packet rules (v2)

1. **Encryption is mandatory.** Every packet after the handshake MUST set
   `ENCRYPTED`; a plaintext sensitive packet on a v2 session is dropped
   (already enforced for downgrade in v1.2+).
2. **Header is authenticated.** The 45-byte frame header is passed as the AEAD
   **AAD** (v1 uses empty AAD). Tampering with `msg_type`/`sequence`/
   `session_id` fails the tag. *(This is the wire change v1 could not make
   without breaking its golden vectors; v2 ships new vectors.)*
3. **Replay window enforced** (already shipped in v1.2): a sequence at or below
   the highest verified is rejected; the window advances only after tag
   verification.

## 4. Server key distribution (the practical crux)

Authentication is only as good as the client's knowledge of `spk_S`. Supported
models, strongest first:

- **Provisioned / pinned (recommended for devices).** `spk_S` is compiled into
  the firmware or set in client config. For the AdaTP Cloud fleet, the control
  plane already knows each node's identity and can hand the pinned key to SDKs
  at credential-issue time. Ideal for industrial/UAV fleets: the device trusts
  exactly one server key.
- **TOFU (trust-on-first-use).** The client records `spk_S` on first connection
  and refuses a *changed* key thereafter (SSH `known_hosts` model). Detects MITM
  from the second connection on.
- **Org root (roadmap).** `spk_S` signed by an organization root key shipped in
  the client — a minimal PKI without full X.509.

A v2-capable client that has a pinned/known key for S **MUST require v2** for
that server and MUST reject a v1 (unsigned) `ServerHello` — this is the
downgrade defense. Clients with no known key MAY use TOFU or fall back to v1
**only** behind TLS.

## 5. Version negotiation & backward compatibility

- `ClientHello.ver` selects the protocol. A v1 client sends `ver=1` and the
  server runs the current unauthenticated flow — **v1 is untouched**, its
  conformance vectors unchanged.
- A v2-aware server offers v2; a v1-only server ignores `ver=2` semantics and a
  v2 client detects the missing signature and aborts (per §4) rather than
  silently downgrading.
- See [`10-versioning.md`](./10-versioning.md) for the version field.

## 6. Security properties (to be proven, not asserted)

The formal model in [`formal/adatp_handshake.pv`](./formal/adatp_handshake.pv)
must establish, against a Dolev-Yao active attacker:

1. **Server authentication** — if C completes a session ostensibly with S, then
   S participated with the same transcript (agreement on `epk_C`, `epk_S`,
   version). Corollary: **no MITM** when `spk_S` is correctly pinned.
2. **Session-key secrecy** — the attacker cannot derive `k`.
3. **Downgrade resistance** — a v2 client with a known key never completes a v1
   exchange with S.
4. It must also **reproduce the v1 MITM** (a sanity check that the model has
   teeth): with the signature removed, authentication fails.

Until that model checks (and, ideally, an independent review or audit —
[`../../ROADMAP.md`](../../ROADMAP.md) tier 9), v2 is a *design*, and the honest
posture remains **TLS-mandatory** ([`08-security.md`](./08-security.md)).

## 7. Implementation plan (after verification, coordinated)

1. Model checks in ProVerif (+ optional Tamarin) → design frozen, vectors drawn.
2. Server: wire `core/src/crypto/ed25519.rs` (already present, unused) into the
   handshake behind `ver` negotiation; v1 path untouched; add v2 conformance
   vectors.
3. One reference SDK (JS **and** C — the C one proves the MCU claim) implements
   v2 + key pinning; measure the added handshake cost on an STM32.
4. Remaining SDKs follow, each replaying the new vectors.
5. Only then update the security claims and README.

*Nothing in steps 2–5 happens before step 1.*
