# Conformance assets

`vectors/adatp-v1-vectors.json` is the machine-readable golden vector set
(also embedded in docs/spec/appendix-test-vectors.md). The Rust replay runs
with `cargo test -p adatp-core` (see core/tests/conformance.rs). The
Node.js/Python replay runners and the live integration suites require the
sibling SDK checkouts — see docs/testing/README.md for the workspace layout.
