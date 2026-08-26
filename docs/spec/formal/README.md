# Formal models — AdaTP handshake

Symbolic (Dolev-Yao) models of the AdaTP key exchange in the applied pi
calculus, for [ProVerif](https://bblanche.gitlabpages.inria.fr/proverif/).

They exist to make the security of the **proposed v2 authenticated handshake**
([`../12-authenticated-handshake.md`](../12-authenticated-handshake.md)) a thing
that can be *checked*, not asserted — the free half of the "prove it" step on
the [roadmap](../../../ROADMAP.md) (TLS 1.3, Signal and WireGuard were all
modeled this way).

## ✅ Status — run and passing (full output in [`RESULTS.md`](./RESULTS.md))

Both models were executed with ProVerif and give the expected results: v1's
agreement query is **false** (ProVerif reconstructs the active MITM and breaks
secrecy), and v2's secrecy and injective-agreement queries are both **true**
(no MITM, given a correctly pinned server key).

This is the symbolic — "prove it, don't claim it" — half, and it is **necessary
but not sufficient**: it assumes perfect cryptographic primitives, models key
pinning as perfect, and does **not** replace review by a symbolic-analysis
expert or an independent audit ([roadmap](../../../ROADMAP.md) tier 9). It also
does not yet include a mixed-version downgrade query (see limitations). The
security *claim* still waits on implementation + those human steps.

## Files

| File | Models | Result (ProVerif) |
| :-- | :-- | :-- |
| `adatp_v1_handshake.pv` | v1: unauthenticated X25519 | Agreement **false**, secrecy **false** — ProVerif reconstructs the active MITM. ✓ (as expected) |
| `adatp_v2_handshake.pv` | v2: + Ed25519 server signature over the transcript, client pins `spk_S` | Agreement **true**, secrecy **true** — no MITM. ✓ |
| `adatp_v2_downgrade.pv` | v2 **and** a v1 (unsigned) server sharing one identity + active attacker | Client completes **only** via v2 (agreement **true**), secrecy **true** — no silent downgrade. ✓ |

The contrast is the point: same primitives, and the *only* difference is the
server signature + the client's identity check — so if v1 fails and v2 holds,
the signature is demonstrably what closes the gap.

## Run

```bash
# Debian/Ubuntu: apt-get install proverif   — or build from source / opam
proverif adatp_v1_handshake.pv    # expect: the agreement query is false (attack trace printed)
proverif adatp_v2_handshake.pv    # expect: both queries true
proverif adatp_v2_downgrade.pv    # expect: both queries true (client completes only via v2)
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
- Downgrade (v2-client ↔ v1-server) is **now encoded** as a dedicated model
  (`adatp_v2_downgrade.pv`) and passes: a pinning client completes only via v2
  even with a v1 server on the same attacker-controlled network. The server-side
  `ADATP_MIN_PROTOCOL_VERSION` floor enforces the same policy operationally.

An independent audit (ROADMAP tier 9) remains the paid, human tier — the
symbolic models are necessary, not sufficient.
