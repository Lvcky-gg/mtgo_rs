# Surface failed deck deletions

Deck deletion now reports database errors through the existing deck-screen error
display instead of swallowing them and clearing selection. Successful deletion
clears selection only if that deck was selected, preserving details of another
selected deck. Confirmation resets after the attempt.

This change adds no database operations or deletion paths; it corrects handling
of the existing explicitly confirmed delete action. Changes remain uncommitted.
