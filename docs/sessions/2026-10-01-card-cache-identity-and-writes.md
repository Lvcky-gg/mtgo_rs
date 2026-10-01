# Preserve card-image cache identity and concurrent writes

Card cache files now use SHA-256 of the full real card name. Punctuation variants
no longer share a cache path, and arbitrarily long Unicode names produce bounded
68-byte filenames. The app directly uses the sha2 version already used by mtg-net.
Old cache files are retained but are not reused because their names cannot
unambiguously identify their images; requested art is downloaded under new names.

Cache writes now use exclusive, per-process/per-write temporary files instead of
a shared .part path. Concurrent workers cannot truncate one another's in-progress
files. Files are closed before rename, and failed writes/renames clean up their
own temporary files.

Regressions check punctuation distinctions, long names, stable paths, and eight
concurrent writes leaving one complete image with no temporary leftovers.

Validation: all 13 art tests and app all-target Clippy with dependency lints
disabled and warnings denied pass. Changes remain uncommitted.
