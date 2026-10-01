# Keep the hosted lobby open after unaccepted setup failures

`mtg-app/src/net.rs` now waits through encrypted setup and lobby negotiation inside
one accept loop. Invalid Noise handshakes and rejected invites report the failure
and resume waiting, preserving the issued invite. Cancellation and invite expiry
are checked between attempts; cancellation is also checked before entering play.
The existing per-I/O setup timeout still bounds stalled peers.

A successful Join consumes the invite. If the guest disconnects or setup fails
from that point onward, hosting ends rather than reopening the single-use invite.
The host seed and commitment remain fixed across unaccepted attempts.

Four loopback regression tests cover invalid Noise followed by a successful join,
a rejected invite followed by the legitimate invite, disconnect after acceptance,
and cancellation before accept. All four pass. App compilation and all-target
Clippy with warnings denied pass; diff whitespace checks pass. Concurrent engine
and compiler work was left untouched.

The full app suite also exposed a stale builder assertion from the earlier shared
copy-limit change. Updated it to expect 21 Stone Bears across main deck and
sideboard rather than five in the main deck only. No builder production code
changed.

Final validation: all 109 app library tests pass, app all-target Clippy with
warnings denied passes, and `git diff --check` passes.
