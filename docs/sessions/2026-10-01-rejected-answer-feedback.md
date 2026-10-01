# Explain rejected game answers

The GUI records the answered question ID. If the engine re-asks that ID, the
prompt displays That answer was not legal. Choose again. Accepted answers that
advance to a new question do not show the message. Undo submissions are excluded
because restoring a question does not imply a rejected ordinary answer.

The headless regression checks repeated-question feedback and its rendered text,
clearing on a new question, and no false feedback after Undo.

Validation: all 19 UI tests, app all-target Clippy with dependency lints disabled
and warnings denied, and diff checks pass. Initial checks encountered the concurrent
CardKey/FaceRow migration; successful checks followed updated call sites. Other
workers' changes were preserved. Changes remain uncommitted.
