# Enforce guest lobby message ordering and host identity

`guest_handshake` now requires one Accepted message naming the invited host,
followed by Reveal. It refuses a mismatched accepting identity before disclosing
the guest seed, rejects Reveal before the host commits, and rejects a repeated
Accepted message rather than replacing the recorded host commitment.

Three scripted in-memory host regressions cover these cases and verify what the
guest sends. All 87 transport-enabled network tests pass (73 library, seven
encryption integration, seven socket integration). Network all-target Clippy with
warnings denied and diff whitespace checks pass. Changes are confined to the wire
layer and its tests; concurrent engine/compiler work was preserved.

App verification also passes: all 115 library tests and both network-match
integration tests, including a hosted game played to completion.
