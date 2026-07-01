//! Error taxonomy for `vault-factory`.
//!
//! Every state-mutating entrypoint returns/panics through one of these
//! variants (never a bare `panic!`), so integrators and off-chain indexers
//! get a stable, documented error code rather than an opaque host trap.
//! Discriminants are part of the on-chain ABI and must never be reordered
//! or reused once shipped.

use soroban_sdk::contracterror;

