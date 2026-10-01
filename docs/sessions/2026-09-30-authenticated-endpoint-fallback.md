# Try invite addresses until the host is authenticated

The guest now tries the next listed direct address when TCP, WebSocket, or Noise
identity setup fails. Previously, any successful WebSocket upgrade ended address
fallback, even if that endpoint could not prove the invited host's identity.
Every successful result still verifies the exact host key in the signed invite.
No lobby Join is sent until authentication succeeds, and lobby negotiation errors
are not retried, preserving single-use invite behavior. Invite validity is
rechecked between address attempts; setup I/O timeouts remain in place.

Three regression tests cover an impostor followed by the real host, refusal when
only an impostor is reachable, and a clear error for no direct addresses. All 112
app library tests pass. `git diff --check` passes. Normal app Clippy encountered a
concurrent engine warning: `finish_resolution` has eight arguments. No engine
files were changed for this task.

A follow-up Clippy check with dependency lints disabled was also blocked by
in-progress renown additions: missing `Event::BecameRenowned`,
`Effect::BecomeRenowned`, and `Condition::Renowned` variants. The passing full app
test result preceded these concurrent edits.
