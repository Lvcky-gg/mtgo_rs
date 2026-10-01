# Bound decoded card images

Card-art decoding now enforces a maximum width and height of 2048 pixels and a
16 MiB decoder allocation budget, in addition to the existing 8 MiB download
limit. Images outside these bounds use the existing card-text fallback.
This applies to cached images as well as downloads.

A regression encodes narrow PNGs just beyond each dimension limit. They are
well under the compressed download limit but are rejected by decoding. Existing
cached-image and symbol-rasterization tests pass.

Validation: all 11 art tests and app all-target Clippy with dependency lints
disabled and warnings denied pass. Decoder allocation limits follow the image
library's support; dimension limits are strict. Changes remain uncommitted.
