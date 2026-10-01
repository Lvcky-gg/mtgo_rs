# Choose cleanup discards and optional modes

DiscardToHandSize now uses the object-selection controls rather than accepting
the policy default through Continue. Players can choose cards through the hand
or named buttons, and confirmation requires exactly the requested discard count.
Only visible objects in the viewer's hand are offered. A regression checks the
candidate zone, player, exact count, and ordinary object-choice bounds.

Optional mode choices with a minimum of zero now use selectable modes and
confirmation even when their maximum is one. This permits choosing no mode;
mandatory single-mode choices retain immediate click-to-answer behavior.

Validation: all three UI selection tests and app all-target Clippy with dependency
lints disabled and warnings denied pass. Changes remain uncommitted.
