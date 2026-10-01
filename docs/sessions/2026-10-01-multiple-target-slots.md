# Preserve multiple target slots in UI answers

Target questions with multiple slots now keep an independent selection per slot
and require confirmation once every nonempty slot has a legal selected target.
Single-slot questions retain immediate click-to-answer behavior. Empty legal
sets produce empty target groups, consistent with the existing policy fallback.

Selection assembly preserves slot order and permits repeated targets when each
slot offers them. Target selections reset between questions and participate in
selection detection and undo. Two regressions cover incomplete and illegal
selections, order, repeated targets, and empty slots.

Validation: both target-selection tests, GUI build, app all-target Clippy with
dependency lints disabled and warnings denied pass. Changes remain uncommitted.
