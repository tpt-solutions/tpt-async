## What does this PR change?

<!-- One or two sentences: the what and the why. Link issues with Fixes #N. -->

## Checklist

- [ ] `cargo xtask ci` passes locally (fmt, clippy, tests, docs, cross-targets, deny)
  - or: `cargo nextest run --workspace --all-features` + `cargo clippy --workspace --all-features -- -D warnings` at minimum
- [ ] Public API changes are documented (`missing_docs` will catch gaps)
- [ ] `CHANGELOG.md` updated under `## [Unreleased]` for user-visible changes
- [ ] New deps pass `cargo deny check` (no GPL/OpenSSL)

## Notes for reviewers

<!-- Anything non-obvious: design choices, follow-up work, test strategy. -->
