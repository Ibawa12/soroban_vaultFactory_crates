//! Integration test harness for `vault-factory`.
//!
//! Every test here spins up a fresh [`Env`], deploys a fresh contract
//! instance via [`setup`], and drives it exclusively through the
//! generated [`VaultFactoryClient`] — the same interface an off-chain
//! caller or another contract would use. Use this file as the template
//! when adding coverage for a newly implemented entrypoint (in
//! particular, `approve`, `execute`, and `deploy_vault` — see the
//! `#[ignore]`d placeholders at the bottom).

#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Address, Env};

use crate::contract::{VaultFactory, VaultFactoryClient};
use crate::errors::VaultError;
use crate::types::{ProposalAction, TransferAction};

/// Number of signers used by [`setup`]'s default vault, and the default
/// M-of-N threshold (2-of-3).
const DEFAULT_THRESHOLD: u32 = 2;
const DEFAULT_TIMELOCK_BLOCKS: u32 = 100;

/// Registers a fresh contract instance and returns a client bound to it
/// along with the generated signer addresses, *without* calling
/// `initialize` — for tests that want to exercise initialization itself.
fn setup_uninitialized(env: &Env) -> (VaultFactoryClient<'static>, soroban_sdk::Vec<Address>) {
    let contract_id = env.register(VaultFactory, ());
    let client = VaultFactoryClient::new(env, &contract_id);
    let signers = vec![
        env,
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    ];
    (client, signers)
}

/// Registers a fresh contract instance, initializes it as a 2-of-3 vault
/// with a [`DEFAULT_TIMELOCK_BLOCKS`]-ledger timelock, and returns the
/// client plus its signer set.
fn setup(env: &Env) -> (VaultFactoryClient<'static>, soroban_sdk::Vec<Address>) {
    let (client, signers) = setup_uninitialized(env);
    client.initialize(&signers, &DEFAULT_THRESHOLD, &DEFAULT_TIMELOCK_BLOCKS);
    (client, signers)
}

