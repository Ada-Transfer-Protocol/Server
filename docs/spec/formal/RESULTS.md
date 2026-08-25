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

## Caveats (why this is necessary, not sufficient)
- Symbolic (Dolev-Yao) model: perfect crypto assumed. Does NOT cover X25519 small-subgroup/point-validation, Ed25519 cofactor/malleability, nonce reuse, or the empty-vs-header AAD — those are implementation concerns.
- Key distribution modeled as perfect pinning; TOFU first-contact and the mixed-version downgrade query are not yet modeled (see README).
- Independent review + an audit (ROADMAP tier 9) remain the paid, human step.
