# Targeted-player counts

Resolving effects can count permanents controlled by a targeted player or opponent:
“draw a card for each tapped creature target opponent controls”, “gain 2 life for
each Mountain target opponent controls”, and scaled creature bonuses.

The compiler allocates the player target before parsing its controlled-object
filter, preserves preceding object targets, and only commits the new target if
the count consumes the player qualifier. The temporary controller target is scoped
to the count. Static counts remain unable to allocate targets. Existing engine
count evaluation uses current battlefield state at resolution; an illegal sole
player target makes the spell fail to resolve.

Three gameplay regressions cover zero through three tapped creatures, exclusion
of the caster's creatures and noncreatures, targeting yourself with target-player
wording, distinct object/player target slots, a land destroyed in response, and a
player acquiring hexproof before resolution.

Validation: 1,208 engine/Oracle tests passed, 38 ignored. Workspace all-target,
all-feature Clippy and formatting checks pass. Live audit: 16,393 / 34,913 playable
as printed (47.0%), up five from the 16,388 snapshot taken at the start of this
change. Concurrent coin-flip/clash work raised the earlier reported 16,351 total.
