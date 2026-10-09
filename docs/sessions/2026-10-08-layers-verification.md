# Layer system golden tests (CR 613)

The verification roadmap listed layers without direct golden propositions. Eight
were written in `crates/mtg-verify/tests/layers.rs` from the CR 613 text (2026-09-25
release) before reading the layer implementation's behavior. All effects come from
real compiled rules text activated in an ordinary game: base P/T setting, switching,
+1/+1 counters, pumps, "loses all abilities", control change, a lord, an anthem, a
*/* characteristic-defining creature and a global ability-loss creature.

Two propositions failed: a permanent that loses all its abilities still generated its
static abilities' effects, so a silenced lord kept pumping, and a silenced */* CDA
creature kept its P/T instead of dying as a 0/0. `layers.rs` documented this as not
modelled. `layers::effects` now drops static effects whose source is covered by a
"loses all abilities" effect, evaluated against printed characteristics like every
affected set, while the removing ability itself keeps applying (CR 613.6).

Passing propositions: base-setting before counters and modifications before switching
in either timestamp order, 7a < 7b < 7c, control changes between anthems, and layer-6
timestamp order for gaining/losing abilities.

Evidence: 8 golden tests; two new production mutants; a fresh full campaign kills
15/15 (including two concurrent Codex targeting mutants); 22-check PR campaign passes;
workspace 1,901 tests pass. The one failing test is Codex's concurrent targeting
change (`optional_target_prompts`), reported in coordination.

Open: dependency ordering (CR 613.8; `dependency_sort` is a no-op), copy effects,
text/type/color layers, and ability removal decided by characteristics another effect
changed. Tests are Builder-authored; no primitive promoted.

## Second increment: color layer reach

Two more propositions (CR 613.1e): a creature that becomes blue gets "blue creatures
get +1/+1", a blue one made red stops getting it, and one made black gets "black
creatures you control have flying". Both failed: `ordered_effects` decided every
dynamic effect's reach once, up front, from printed characteristics. `compute` now
decides it as each effect's layer comes up, reading the object itself as computed so
far (`SoFar`) and every other object as printed. One more production mutant. Type
changes (layer 4) couldn't be tested: the compiler doesn't yet read most type-changing
statics ("Nonbasic lands are Mountains", "All creatures are black").
