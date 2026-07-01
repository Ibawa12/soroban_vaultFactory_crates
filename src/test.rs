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

#[test]
fn initialize_succeeds_with_valid_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup_uninitialized(&env);

    client.initialize(&signers, &DEFAULT_THRESHOLD, &DEFAULT_TIMELOCK_BLOCKS);
    // A second call must fail: the vault is already initialized.
    let result = client.try_initialize(&signers, &DEFAULT_THRESHOLD, &DEFAULT_TIMELOCK_BLOCKS);
    assert_eq!(result, Err(Ok(VaultError::AlreadyInitialized)));
}

#[test]
fn initialize_rejects_empty_signer_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signers) = setup_uninitialized(&env);
    let empty: soroban_sdk::Vec<Address> = vec![&env];

    let result = client.try_initialize(&empty, &1, &DEFAULT_TIMELOCK_BLOCKS);
    assert_eq!(result, Err(Ok(VaultError::EmptySignerSet)));
}

#[test]
fn initialize_rejects_duplicate_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signers) = setup_uninitialized(&env);
    let signer = Address::generate(&env);
    let duplicated = vec![&env, signer.clone(), signer];

    let result = client.try_initialize(&duplicated, &1, &DEFAULT_TIMELOCK_BLOCKS);
    assert_eq!(result, Err(Ok(VaultError::DuplicateSigner)));
}

#[test]
fn initialize_rejects_threshold_above_signer_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup_uninitialized(&env);

    let result = client.try_initialize(&signers, &(signers.len() + 1), &DEFAULT_TIMELOCK_BLOCKS);
    assert_eq!(result, Err(Ok(VaultError::InvalidThreshold)));
}

#[test]
fn initialize_rejects_timelock_beyond_ceiling() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup_uninitialized(&env);

    let result = client.try_initialize(
        &signers,
        &DEFAULT_THRESHOLD,
        &(crate::types::MAX_TIMELOCK_LEDGERS + 1),
    );
    assert_eq!(result, Err(Ok(VaultError::InvalidTimelockDuration)));
}

#[test]
fn configure_spending_limit_by_current_signer_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let asset = Address::generate(&env);

    client.configure_spending_limit(&signers.get_unchecked(0), &asset, &1_000_000i128, &17_280u32);
}

#[test]
fn configure_spending_limit_rejects_non_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signers) = setup(&env);
    let asset = Address::generate(&env);
    let stranger = Address::generate(&env);

    let result =
        client.try_configure_spending_limit(&stranger, &asset, &1_000_000i128, &17_280u32);
    assert_eq!(result, Err(Ok(VaultError::SignerNotFound)));
}

#[test]
fn propose_by_signer_returns_incrementing_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let action = ProposalAction::Transfer(TransferAction {
        asset: Address::generate(&env),
        to: Address::generate(&env),
        amount: 500,
    });

    let first_id = client.propose(&signers.get_unchecked(0), &action.clone());
    let second_id = client.propose(&signers.get_unchecked(1), &action);
    assert_eq!(second_id, first_id + 1);
}

#[test]
fn propose_rejects_non_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signers) = setup(&env);
    let stranger = Address::generate(&env);
    let action = ProposalAction::Transfer(TransferAction {
        asset: Address::generate(&env),
        to: Address::generate(&env),
        amount: 500,
    });

    let result = client.try_propose(&stranger, &action);
    assert_eq!(result, Err(Ok(VaultError::SignerNotFound)));
}

// --- Contributor-facing entrypoint placeholders ----------------------------
//
// `approve`, `execute`, and `deploy_vault` currently panic via `todo!()`
// (see `contract.rs`). These tests sketch the intended flow and are marked
// `#[ignore]` so CI stays green; once an issue lands an implementation,
// remove the `#[ignore]` and flesh out the assertions.

#[test]
#[ignore = "VaultFactory::approve is an open contributor issue"]
fn approve_transitions_proposal_to_ready_once_threshold_met() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let action = ProposalAction::Transfer(TransferAction {
        asset: Address::generate(&env),
        to: Address::generate(&env),
        amount: 500,
    });
    let id = client.propose(&signers.get_unchecked(0), &action);

    client.approve(&signers.get_unchecked(0), &id);
    client.approve(&signers.get_unchecked(1), &id);
    // Once implemented: assert the proposal's status is now `Ready`.
}

#[test]
#[ignore = "VaultFactory::execute is an open contributor issue"]
fn execute_rejects_before_timelock_elapses() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let action = ProposalAction::Transfer(TransferAction {
        asset: Address::generate(&env),
        to: Address::generate(&env),
        amount: 500,
    });
    let id = client.propose(&signers.get_unchecked(0), &action);
    client.approve(&signers.get_unchecked(0), &id);
    client.approve(&signers.get_unchecked(1), &id);

    let result = client.try_execute(&signers.get_unchecked(0), &id);
    assert_eq!(result, Err(Ok(VaultError::TimelockNotExpired)));
}

#[test]
#[ignore = "VaultFactory::deploy_vault is an open contributor issue"]
fn deploy_vault_returns_a_freshly_initialized_child_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    // Once implemented, replace this with a real uploaded WASM hash via
    // `env.deployer().upload_contract_wasm(WASM_BYTES)` (e.g. this same
    // contract's own compiled output, for a self-similar factory).
    let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
    let salt = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    let _child = client.deploy_vault(
        &wasm_hash,
        &salt,
        &signers,
        &DEFAULT_THRESHOLD,
        &DEFAULT_TIMELOCK_BLOCKS,
    );
}
