# Cancel armed deck deletion

Armed deck-delete prompts now include a Cancel button. Selecting another deck,
opening an existing deck for editing, creating a new deck, or leaving for the menu
also disarms the prompt. Confirmed deletion retains its existing database path
and error handling.

Validation: GUI build, app all-target Clippy with dependency lints disabled and
warnings denied, and diff checks pass. Changes remain uncommitted.
