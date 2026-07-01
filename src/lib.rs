//! `soroban-VaultFactory` — a multi-sig & timelock vault factory for
//! Soroban.
//!
//! Module layout:
//! - [`types`]: plain data structures (`VaultConfig`, `Proposal`,
//!   `SpendingLimit`, ...), free of storage/host side effects.
//! - [`errors`]: the contract's complete `#[contracterror]` taxonomy.
//! - [`storage`]: typed, storage-class-explicit persistence over the
//!   `types` above.
//! - [`contract`]: the `#[contract]` entrypoints themselves.
#![no_std]
// The package name `soroban-VaultFactory` is intentionally mixed-case (it
// mirrors the GitHub repository name); rustc's generated crate identifier
// (`soroban_VaultFactory`) trips the default `non_snake_case` lint, which
// is purely cosmetic here and not worth renaming the package over.
#![allow(non_snake_case)]

pub mod contract;
pub mod errors;
pub mod storage;
pub mod types;

#[cfg(test)]
mod test;

pub use contract::VaultFactory;
pub use errors::VaultError;
