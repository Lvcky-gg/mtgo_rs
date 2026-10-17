# Deathtouch as a removal response

Response scoring now considers printed deathtouch on the damage source when
matching fixed positive damage to creature threats. A one-damage ability can
therefore answer a large recurring draw engine. Target selection and proactive
response selection share this scoring. Indestructible and visible damage shields
still reject the answer; planeswalker and player damage still require sufficient
numeric damage.

Recognized damage responses now require SelfSource as their damage source.
Effects that make another object deal damage remain on the fallback path, rather
than borrowing the ability source's keywords.

A compiled-card game matrix tests a deathtouch pinger removing a 5/5 draw engine,
a normal pinger passing, and the deathtouch pinger passing against indestructible.
The successful case resolves in the engine and checks that the threat dies and
the pinger taps. Policy58, headless bot1, compiled1496 (15 ignored), strict policy
all-target Clippy, and formatting/diff checks pass. Arena results are recorded in
the coordination entry. Final-code Demo baseline: 50%, 16 games, no
adjudications, 2.3s. Search comparison was built before the final SelfSource
recognition guard: unchanged 68.8% ±17.9%, +137 Elo [+6,+325],
132.5s, 3096 decisions, 39 overrides. No overall strength gain established.

Limit: printed keywords only; conditional or granted deathtouch is not inferred.
No engine or search core/eval changes.
