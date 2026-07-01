//! Main contract entrypoints: initialization, spending-limit
//! configuration, proposal creation, and the (currently skeletal) approval,
//! execution, and factory-deployment flows.
//!
//! ---
//! ### Contributor note
//! [`VaultFactory::initialize`], [`VaultFactory::configure_spending_limit`],
//! and [`VaultFactory::propose`] are fully implemented and should be used
//! as the reference pattern (auth check -> validate -> load/mutate state
//! via [`crate::storage`] -> persist) for the three `todo!()` entrypoints
//! below, each tracked as an open-source issue:
//!
//! | Function        | Suggested issue difficulty |
//! |------------------|-----------------------------|
//! | [`VaultFactory::approve`]     | Medium (M-of-N auth loop over existing signer set) |
//! | [`VaultFactory::execute`]     | High (timelock + spending-limit + action dispatch, security-sensitive) |
//! | [`VaultFactory::deploy_vault`] | High (cross-contract deployer, WASM hash validation) |

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};

use crate::errors::VaultError;
use crate::storage;
use crate::types::{
    ProposalAction, ProposalStatus, SpendingLimit, VaultConfig, MAX_SIGNERS, MAX_TIMELOCK_LEDGERS,
};

#[contract]
pub struct VaultFactory;

