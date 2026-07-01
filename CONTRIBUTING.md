# Contributing to soroban-VaultFactory

Thanks for considering a contribution. This contract moves (or will move,
once `execute` is implemented) real funds under real multisig/timelock
guarantees — the bar for review here is correspondingly higher than for a
typical open-source PR. This document explains how to contribute
effectively and safely.

## Before you start

1. **Check for an existing issue or claim.** If you're picking up
   `approve`, `execute`, or `deploy_vault` (see the
   [README's Open Issues table](README.md#open-issues)), comment on (or
   open) the corresponding GitHub issue first so two people don't
   duplicate the same work.
2. **Read the doc-comment before writing code.** Every `todo!()` entrypoint
   in [`src/contract.rs`](src/contract.rs) has a doc-comment enumerating
   the exact required steps, in order, and every error variant it must be
   able to return. That doc-comment is the spec. If you believe it's wrong,
   incomplete, or unsafe as written, open a discussion issue before
   implementing something different — for `execute` especially, a
   "better" undiscussed deviation is a bigger risk than a slower discussed
   one.
3. **Small PRs over big ones.** One entrypoint, one bug fix, or one test
   improvement per PR. Don't bundle `approve` and `execute` into a single
   PR even though they're related — they should be reviewable (and
   revertable) independently.

## Development setup

```bash
git clone <your-fork-url>
cd soroban-VaultFactory
rustup target add wasm32-unknown-unknown   # rust-toolchain.toml pins this
cargo test                                  # Env-based integration tests
cargo build --target wasm32-unknown-unknown --release   # the deployable artifact
```

