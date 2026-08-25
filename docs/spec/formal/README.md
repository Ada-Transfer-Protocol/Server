# Formal models — AdaTP handshake

Symbolic (Dolev-Yao) models of the AdaTP key exchange in the applied pi
calculus, for [ProVerif](https://bblanche.gitlabpages.inria.fr/proverif/).

They exist to make the security of the **proposed v2 authenticated handshake**
([`../12-authenticated-handshake.md`](../12-authenticated-handshake.md)) a thing
that can be *checked*, not asserted — the free half of the "prove it" step on
the [roadmap](../../../ROADMAP.md) (TLS 1.3, Signal and WireGuard were all
modeled this way).

## ⚠️ Status

**These models have NOT been run** — `proverif` was not available where they
were written. They are a **starting point that needs (a) to actually pass, and
(b) review by someone fluent in symbolic analysis** before any claim rests on
them. A model that "verifies" because it is mis-modeled is worse than none.
Treat the expected results below as *hypotheses to confirm*, not facts.

## Files

| File | Models | Expected (to confirm) |
| :-- | :-- | :-- |
| `adatp_v1_handshake.pv` | v1: unauthenticated X25519 | Agreement query **FALSE** — ProVerif finds the active MITM. Secrecy likely broken. |
| `adatp_v2_handshake.pv` | v2: + Ed25519 server signature over the transcript, client pins `spk_S` | Agreement query **TRUE** (no MITM); `secretMsg` stays secret. |

The contrast is the point: same primitives, and the *only* difference is the
server signature + the client's identity check — so if v1 fails and v2 holds,
the signature is demonstrably what closes the gap.

## Run

```bash
# Debian/Ubuntu: apt-get install proverif   — or build from source / opam
proverif adatp_v1_handshake.pv    # expect: the agreement query is false (attack trace printed)
proverif adatp_v2_handshake.pv    # expect: both queries true
```

## Modeling notes & known limitations

- X25519 is the standard DH abstraction (`dhexp` + the commuting equation); it
  does **not** capture small-subgroup / point-validation issues — those are an
  implementation concern, not a symbolic one.
- Ed25519 is the perfect-unforgeability signature abstraction (no
  malleability / key-substitution modeling). Real Ed25519 has subtleties
  (cofactor, batch verification) that symbolic models do not see.
- HKDF and AES-GCM are perfect (KDF = free function, AEAD = `senc`/`sdec`); the
  model does not reason about nonce reuse or the empty-vs-header AAD — those are
  covered in prose in the spec and must be checked in code.
- Key distribution is modeled as **perfect pinning** (the client already holds
  `spk_S`). TOFU and the first-contact window are **not** modeled and remain a
  real-world risk (see the spec §4).
- Downgrade (v2-client ↔ v1-server) is argued in prose in the spec §5, not yet
  encoded as a mixed-version ProVerif query — a good next addition.

Confirming these models (and adding the mixed-version downgrade query) is a
concrete, no-cost roadmap task; an independent audit remains the paid tier.
