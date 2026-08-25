# ProVerif verification results

Tool: Proverif 2.05. Cryptographic protocol verifier, by Bruno Blanchet, Vincent Cheval, and Marc Sylvestre
Run: 2026-08-26 (in the authoring environment)

## adatp_v1_handshake.pv (unauthenticated — control)
```
RESULT not attacker(secretMsg[]) is false.
RESULT inj-event(ClientDone(eC,eS)) ==> inj-event(ServerRan(eC,eS)) is false.
RESULT (even event(ClientDone(eC,eS)) ==> event(ServerRan(eC,eS)) is false.)
```
Interpretation: secrecy is broken and agreement fails — ProVerif reconstructs the active MITM. The model has teeth.

## adatp_v2_handshake.pv (authenticated)
```
RESULT not attacker(secretMsg[]) is true.
RESULT inj-event(ClientDone(eC,eS)) ==> inj-event(ServerRan(eC,eS)) is true.
```
Interpretation: session-key secrecy holds and the client injectively agrees with the server on the transcript — no MITM — against a Dolev-Yao active attacker, given a correctly pinned server key.

## adatp_v2_downgrade.pv (mixed-version downgrade resistance)
```
RESULT not attacker(secretMsg[]) is true.
RESULT inj-event(ClientDone(eC,eS)) ==> inj-event(ServerV2Ran(eC,eS)) is true.
```
Interpretation: with a v2 (signing) server AND a v1 (unsigned) server sharing the same identity on the same attacker-controlled network, a pinning client that requires a signature completes **only** with a genuine v2 server run — never the v1 path. The pin + mandatory signature provably defeats a silent version downgrade, and secrecy still holds. (Complements the server-side `ADATP_MIN_PROTOCOL_VERSION` floor, which enforces the same policy operationally.)

## Caveats (why this is necessary, not sufficient)
- Symbolic (Dolev-Yao) model: perfect crypto assumed. Does NOT cover X25519 small-subgroup/point-validation, Ed25519 cofactor/malleability, or nonce reuse — those are implementation concerns.
- The header-as-AAD binding is now implemented in code (v2 sessions) and checked by tests, but the symbolic models here abstract AEAD as perfect and do not separately model it.
- Key distribution is modeled as perfect pinning; TOFU first-contact is not modeled.
- Independent review + an audit (ROADMAP tier 9) remain the paid, human step.
