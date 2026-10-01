# Explain current deck support coverage

Deck details and match setup now label support coverage as a count of unique
cards, matching the deduplicated whole-card coverage calculation. The details
hover explains that the check covers full rules text, layouts, and every face,
and directs players to builder explanations for unsupported rules. It no longer
claims that only vanilla and keyword cards work or that other cards are castable
with missing abilities. The coverage function comment was updated accordingly.

Validation: package formatting, app all-target Clippy with dependency lints
disabled and warnings denied, and diff checks pass. This changes explanatory
text only; no new tests were added. Changes remain uncommitted.
