CR 613 (layers) propositions execute in `crates/mtg-verify/tests/layers.rs`. Every
effect comes from rules text compiled by the real compiler and is created through
ordinary activations; expected values are worked out from the rules, not recorded.

- 613.4b/c: base-setting applies before counters, in either timestamp order.
- 613.4c/d: switching applies after modifications, in either timestamp order.
- 613.4a–c: a characteristic-defining ability (7a) is overridden by base-setting
  (7b); anthems (7c) still apply on top.
- 613.1f/g: a permanent that loses all abilities stops generating its static
  abilities' effects (a lord's anthem), but a resolved pump from another source stays.
- 604.3/613.1f: a */* creature that loses its characteristic-defining ability is 0/0
  and dies.
- 613.1b: control changes (layer 2) move a creature between "creatures you control"
  anthems.
- 613.3: ability adding and removing in layer 6 follow timestamp order.
- 613.6: a creature's own "all creatures lose all abilities" keeps applying although
  it removes itself.
- 613.1e: a color change (layer 5) decides "blue creatures get +1/+1" (layer 7) and
  "black creatures you control have flying" (layer 6), both ways.

Not yet covered: dependencies (613.8) — the engine's `dependency_sort` is a
documented no-op — copy effects (layer 1), text changes (layer 3) and type changes
(layer 4) — most type-changing static text doesn't compile yet. Builder-authored from the CR text (2026-09-25 release); the
two ability-loss propositions failed before the engine fix.
