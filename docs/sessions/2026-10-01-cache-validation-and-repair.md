# Validate image caches and repair invalid entries

Card and symbol loaders now decode cached data before treating it as a hit.
Invalid caches fall through to the existing download path. Downloaded data is
decoded before it is written, preventing invalid responses from poisoning future
loads. Each image is decoded once on the worker and passed directly to the UI.

Cache reads are bounded to the existing 8 MiB compressed-image limit, so an
oversized local file cannot cause an unbounded read. Valid image/symbol cache hits
still require no network. Invalid existing files are preserved until valid data
successfully replaces them.

A regression covers invalid-download preservation, corrupted cache recovery,
reuse without downloading after recovery, and oversized-cache recovery. Existing
cached card and symbol tests check the decoded outputs.

Validation: all 14 art tests, app all-target Clippy with dependency lints disabled
and warnings denied, and diff checks pass. Changes remain uncommitted.
