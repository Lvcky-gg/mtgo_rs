# Card coverage — 2026-10-14

Continued from round 4 using the same `/tmp/card-coverage.sqlite` catalog snapshot.
Coverage rises from 17,592 to 17,597 / 34,914 (50.4%). The complete card-name
comparison gains Evaporate, Honorable Scout, Ignite Disorder, Kithkin Zealot, and
Sacred Knight, with zero losses. The name-check command retains its existing
five-card difference (17,602 versus 17,597).

Color adjectives now accept `and/or` as a union and `and` as an intersection.
An object with both colors counts once for a union; an intersection requires both
colors but permits additional colors. These use the existing engine filters.

Three compiled gameplay regressions cover Honorable Scout's targeted opponent
count, Sacred Knight's blocking restriction, and a two-color anthem requiring
both colors. Each includes single-color, matching multicolor, and colorless
objects; the anthem also includes an object with a third color.

Validation: targeted compiled tests, `cargo test --workspace --quiet`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`,
and `git diff --check`. Workspace tests run outside the sandbox to permit their
existing loopback networking tests. Changes remain uncommitted.
