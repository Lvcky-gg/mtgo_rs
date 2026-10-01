# Allow the full legal range for X

ChooseX previously rendered only values from the minimum through minimum + 12,
making larger legal values inaccessible. It now keeps direct buttons for ranges
of up to 13 values and uses a bounded numeric editor with confirmation for larger
ranges. Players can type a value or drag the numeric control.

The edited value resets between questions and is clamped to the current legal
bounds. Range-size arithmetic uses saturating subtraction, removing the previous
minimum + 12 overflow at large u32 values. Small-range buttons wrap when needed.

Validation: GUI build, app all-target Clippy with dependency lints disabled and
warnings denied, and diff checks pass. No interactive visual inspection was
performed. Changes remain uncommitted.
