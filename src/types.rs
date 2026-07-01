//! Core data structures for the vault: signer/threshold configuration,
//! spending limits, and the proposal lifecycle.
//!
//! Everything here is a plain `#[contracttype]` value type — no storage
//! access or `Env`-side-effecting logic lives in this module, so the
//! shapes themselves can be constructed and asserted on in ordinary unit
//! tests without spinning up a host `Env`. Persistence lives in
//! [`crate::storage`].

use soroban_sdk::{contracttype, Address, Bytes, Symbol, Vec};

/// Hard ceiling on `timelock_blocks`, expressed in ledgers (~5s each on the
/// Stellar network at time of writing, so this is roughly ~1 year). Chosen
/// to keep `executable_after_ledger` calculations comfortably within `u32`
/// and to prevent a misconfigured vault from locking funds for an
/// effectively unbounded duration.
pub const MAX_TIMELOCK_LEDGERS: u32 = 6_307_200;

