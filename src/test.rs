//! Integration test harness for `vault-factory`.
//!
//! Every test here spins up a fresh [`Env`], deploys a fresh contract
//! instance via [`setup`], and drives it exclusively through the
//! generated [`VaultFactoryClient`] — the same interface an off-chain
//! caller or another contract would use.

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{vec, Address, Env};

use crate::contract::{VaultFactory, VaultFactoryClient};
use crate::errors::VaultError;
use crate::types::{ProposalAction, TransferAction, UpdateSignersAction};

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

/// Registers a Stellar Asset Contract test double, mints `amount` to
/// `to`, and returns its address. Used by the `execute` tests that need a
/// real token to move.
fn setup_funded_token(env: &Env, to: &Address, amount: i128) -> Address {
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(admin);
    let asset = sac.address();
    StellarAssetClient::new(env, &asset).mint(to, &amount);
    asset
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

    client.configure_spending_limit(
        &signers.get_unchecked(0),
        &asset,
        &1_000_000i128,
        &17_280u32,
    );
}

#[test]
fn configure_spending_limit_rejects_non_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signers) = setup(&env);
    let asset = Address::generate(&env);
    let stranger = Address::generate(&env);

    let result = client.try_configure_spending_limit(&stranger, &asset, &1_000_000i128, &17_280u32);
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

// --- approve -----------------------------------------------------------

#[test]
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

    // There's no direct proposal getter exposed on the contract, so the
    // Pending -> Ready transition is observed indirectly: a third signer
    // approving a Ready (no longer Pending) proposal must now fail.
    let result = client.try_approve(&signers.get_unchecked(2), &id);
    assert_eq!(result, Err(Ok(VaultError::ProposalNotPending)));
}

#[test]
fn approve_rejects_non_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let stranger = Address::generate(&env);
    let action = ProposalAction::Transfer(TransferAction {
        asset: Address::generate(&env),
        to: Address::generate(&env),
        amount: 500,
    });
    let id = client.propose(&signers.get_unchecked(0), &action);

    let result = client.try_approve(&stranger, &id);
    assert_eq!(result, Err(Ok(VaultError::SignerNotFound)));
}

#[test]
fn approve_rejects_duplicate_approval_from_the_same_signer() {
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
    let result = client.try_approve(&signers.get_unchecked(0), &id);
    assert_eq!(result, Err(Ok(VaultError::DuplicateApproval)));
}

#[test]
fn approve_rejects_unknown_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);

    let result = client.try_approve(&signers.get_unchecked(0), &999u64);
    assert_eq!(result, Err(Ok(VaultError::ProposalNotFound)));
}

// --- execute -------------------------------------------------------------

#[test]
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
fn execute_rejects_a_still_pending_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let action = ProposalAction::Transfer(TransferAction {
        asset: Address::generate(&env),
        to: Address::generate(&env),
        amount: 500,
    });
    let id = client.propose(&signers.get_unchecked(0), &action);
    // Only one of the two required approvals.
    client.approve(&signers.get_unchecked(0), &id);

    let result = client.try_execute(&signers.get_unchecked(0), &id);
    assert_eq!(result, Err(Ok(VaultError::ProposalNotReady)));
}

#[test]
fn execute_transfers_funds_after_timelock_elapses() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let recipient = Address::generate(&env);
    let asset = setup_funded_token(&env, &client.address, 10_000i128);

    let action = ProposalAction::Transfer(TransferAction {
        asset: asset.clone(),
        to: recipient.clone(),
        amount: 4_000i128,
    });
    let id = client.propose(&signers.get_unchecked(0), &action);
    client.approve(&signers.get_unchecked(0), &id);
    client.approve(&signers.get_unchecked(1), &id);

    env.ledger()
        .set_sequence_number(env.ledger().sequence() + DEFAULT_TIMELOCK_BLOCKS);
    client.execute(&signers.get_unchecked(0), &id);

    let token = soroban_sdk::token::TokenClient::new(&env, &asset);
    assert_eq!(token.balance(&recipient), 4_000i128);
    assert_eq!(token.balance(&client.address), 6_000i128);

    // Executed proposals are terminal: a second execute must fail.
    let result = client.try_execute(&signers.get_unchecked(0), &id);
    assert_eq!(result, Err(Ok(VaultError::ProposalNotReady)));
}

#[test]
fn execute_rejects_transfer_exceeding_the_spending_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let recipient = Address::generate(&env);
    let asset = setup_funded_token(&env, &client.address, 10_000i128);
    client.configure_spending_limit(&signers.get_unchecked(0), &asset, &1_000i128, &17_280u32);

    let action = ProposalAction::Transfer(TransferAction {
        asset: asset.clone(),
        to: recipient,
        amount: 4_000i128, // exceeds the 1,000 limit configured above
    });
    let id = client.propose(&signers.get_unchecked(0), &action);
    client.approve(&signers.get_unchecked(0), &id);
    client.approve(&signers.get_unchecked(1), &id);
    env.ledger()
        .set_sequence_number(env.ledger().sequence() + DEFAULT_TIMELOCK_BLOCKS);

    let result = client.try_execute(&signers.get_unchecked(0), &id);
    assert_eq!(result, Err(Ok(VaultError::SpendingLimitExceeded)));
}

#[test]
fn execute_applies_update_signers_action() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signers) = setup(&env);
    let new_signer = Address::generate(&env);
    let new_signers = vec![&env, signers.get_unchecked(0), new_signer.clone()];

    let action = ProposalAction::UpdateSigners(UpdateSignersAction {
        signers: new_signers,
        threshold: 1,
    });
    let id = client.propose(&signers.get_unchecked(0), &action);
    client.approve(&signers.get_unchecked(0), &id);
    client.approve(&signers.get_unchecked(1), &id);
    env.ledger()
        .set_sequence_number(env.ledger().sequence() + DEFAULT_TIMELOCK_BLOCKS);
    client.execute(&signers.get_unchecked(0), &id);

    // The old third signer is no longer part of the vault...
    let stale_action = ProposalAction::Transfer(TransferAction {
        asset: Address::generate(&env),
        to: Address::generate(&env),
        amount: 1,
    });
    let result = client.try_propose(&signers.get_unchecked(2), &stale_action.clone());
    assert_eq!(result, Err(Ok(VaultError::SignerNotFound)));

    // ...while the newly-added signer now is.
    client.propose(&new_signer, &stale_action);
}

// --- deploy_vault --------------------------------------------------------

/// This crate's own compiled Wasm — used to test `deploy_vault` by having
/// the factory deploy another instance of itself, which is both a
/// realistic exercise of the deployer API and avoids needing a separate
/// fixture contract. Built by the `wasm-build` step the `test` CI job now
/// runs before `cargo test` (see `.github/workflows/ci.yml`); if you're
/// running this test locally, run
/// `cargo build --target wasm32v1-none --release` first. Deliberately
/// `wasm32v1-none`, not `wasm32-unknown-unknown` — see the comment on
/// `targets` in `rust-toolchain.toml`.
const VAULT_FACTORY_WASM: &[u8] =
    include_bytes!("../target/wasm32v1-none/release/soroban_VaultFactory.wasm");

#[test]
fn deploy_vault_returns_a_freshly_initialized_child_vault() {
    let env = Env::default();
    // deploy_vault's cross-contract call into the freshly-deployed
    // child's own `initialize` triggers `signer.require_auth()` calls
    // that aren't tied to this test's root invocation (`deploy_vault`
    // itself) — plain `mock_all_auths()` won't mock those; this variant
    // is needed to authorize auth checks at any call depth.
    env.mock_all_auths_allowing_non_root_auth();
    let (client, signers) = setup(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(VAULT_FACTORY_WASM);
    let salt = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    let child_address = client.deploy_vault(
        &wasm_hash,
        &salt,
        &signers,
        &DEFAULT_THRESHOLD,
        &DEFAULT_TIMELOCK_BLOCKS,
    );
    assert_ne!(child_address, client.address);

    // The child is a fully separate, already-initialized vault instance —
    // proven the same way `initialize_succeeds_with_valid_config` proves
    // it for the top-level `setup()` vault: a second `initialize` call
    // must now fail.
    let child_client = VaultFactoryClient::new(&env, &child_address);
    let result =
        child_client.try_initialize(&signers, &DEFAULT_THRESHOLD, &DEFAULT_TIMELOCK_BLOCKS);
    assert_eq!(result, Err(Ok(VaultError::AlreadyInitialized)));
}

#[test]
fn deploy_vault_reports_deployment_failed_for_an_uninitializable_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signers) = setup(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(VAULT_FACTORY_WASM);
    let salt = soroban_sdk::BytesN::from_array(&env, &[2u8; 32]);
    let empty: soroban_sdk::Vec<Address> = vec![&env];

    // An empty signer set fails the child's own `initialize` validation;
    // deploy_vault must surface that as DeploymentFailed rather than
    // leaving a half-initialized (or un-initializable) vault reachable.
    let result = client.try_deploy_vault(&wasm_hash, &salt, &empty, &1, &DEFAULT_TIMELOCK_BLOCKS);
    assert_eq!(result, Err(Ok(VaultError::DeploymentFailed)));
}
