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

#[contractimpl]
impl VaultFactory {
    /// One-time setup of this contract instance as a single M-of-N vault.
    ///
    /// Validates:
    /// - `signers` is non-empty, contains no duplicate addresses, and does
    ///   not exceed [`MAX_SIGNERS`].
    /// - `threshold` is at least 1 and no greater than `signers.len()`.
    /// - `timelock_blocks` does not exceed [`MAX_TIMELOCK_LEDGERS`].
    ///
    /// Every signer must co-sign the initialization transaction (each
    /// entry in `signers` has `require_auth` invoked on it), so a vault
    /// cannot be stood up with a signer set that hasn't actually consented
    /// to the arrangement.
    ///
    /// # Errors
    /// - [`VaultError::AlreadyInitialized`] if called more than once.
    /// - [`VaultError::EmptySignerSet`], [`VaultError::DuplicateSigner`],
    ///   [`VaultError::InvalidThreshold`], or
    ///   [`VaultError::InvalidTimelockDuration`] on invalid configuration.
    pub fn initialize(
        env: Env,
        signers: Vec<Address>,
        threshold: u32,
        timelock_blocks: u32,
    ) -> Result<(), VaultError> {
        if storage::has_config(&env) {
            return Err(VaultError::AlreadyInitialized);
        }

        validate_signer_set(&signers)?;
        if threshold == 0 || threshold > signers.len() {
            return Err(VaultError::InvalidThreshold);
        }
        if timelock_blocks > MAX_TIMELOCK_LEDGERS {
            return Err(VaultError::InvalidTimelockDuration);
        }

        for signer in signers.iter() {
            signer.require_auth();
        }

        let config = VaultConfig {
            signers,
            threshold,
            timelock_blocks,
            created_at_ledger: env.ledger().sequence(),
        };
        storage::set_config(&env, &config);

        Ok(())
    }

    /// Configures (or replaces) the per-asset [`SpendingLimit`] enforced
    /// against `Transfer` proposals. Any current signer may call this
    /// directly (it is intentionally not itself proposal-gated, since it
    /// only ever *tightens or redefines* a ceiling, never moves funds).
    ///
    /// # Errors
    /// - [`VaultError::NotInitialized`] if `initialize` has not run.
    /// - [`VaultError::SignerNotFound`] if `caller` is not a current
    ///   signer.
    pub fn configure_spending_limit(
        env: Env,
        caller: Address,
        asset: Address,
        limit_per_period: i128,
        period_ledgers: u32,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        let config = storage::get_config(&env)?;
        if !config.signers.contains(&caller) {
            return Err(VaultError::SignerNotFound);
        }

        let limit = SpendingLimit {
            asset: asset.clone(),
            limit_per_period,
            period_ledgers,
        };
        storage::set_spending_limit(&env, &asset, &limit);

        Ok(())
    }

    /// Submits a new [`crate::types::Proposal`] for the given `action`.
    ///
    /// The proposal starts in [`ProposalStatus::Pending`] with zero
    /// approvals — submitting is distinct from approving, so a proposer
    /// who wants their own approval counted must separately call
    /// [`VaultFactory::approve`], same as any other signer. Its
    /// `executable_after_ledger` is snapshotted from the vault's current
    /// `timelock_blocks` at creation time.
    ///
    /// # Errors
    /// - [`VaultError::NotInitialized`] if `initialize` has not run.
    /// - [`VaultError::SignerNotFound`] if `proposer` is not a current
    ///   signer.
    pub fn propose(env: Env, proposer: Address, action: ProposalAction) -> Result<u64, VaultError> {
        proposer.require_auth();
        let config = storage::get_config(&env)?;
        if !config.signers.contains(&proposer) {
            return Err(VaultError::SignerNotFound);
        }

        let id = storage::next_proposal_id(&env);
        let current_ledger = env.ledger().sequence();
        let proposal = crate::types::Proposal {
            id,
            proposer,
            action,
            approvals: Vec::new(&env),
            created_at_ledger: current_ledger,
            executable_after_ledger: current_ledger + config.timelock_blocks,
            status: ProposalStatus::Pending,
        };
        storage::set_proposal(&env, &proposal);

        Ok(id)
    }

    /// Records `signer`'s approval of `proposal_id`, transitioning the
    /// proposal from [`ProposalStatus::Pending`] to
    /// [`ProposalStatus::Ready`] once `VaultConfig::threshold` distinct
    /// approvals have been collected.
    ///
    /// # Errors
    /// - [`VaultError::NotInitialized`]
    /// - [`VaultError::SignerNotFound`]
    /// - [`VaultError::ProposalNotFound`]
    /// - [`VaultError::ProposalNotPending`]
    /// - [`VaultError::DuplicateApproval`]
    pub fn approve(env: Env, signer: Address, proposal_id: u64) -> Result<(), VaultError> {
        signer.require_auth();
        let config = storage::get_config(&env)?;
        if !config.signers.contains(&signer) {
            return Err(VaultError::SignerNotFound);
        }

        let mut proposal = storage::get_proposal(&env, proposal_id)?;
        if proposal.status != ProposalStatus::Pending {
            return Err(VaultError::ProposalNotPending);
        }
        if proposal.approvals.contains(&signer) {
            return Err(VaultError::DuplicateApproval);
        }

        proposal.approvals.push_back(signer);
        if proposal.approvals.len() >= config.threshold {
            proposal.status = ProposalStatus::Ready;
        }
        storage::set_proposal(&env, &proposal);

        Ok(())
    }

    /// Executes `proposal_id` once it has reached
    /// [`ProposalStatus::Ready`] and its timelock has elapsed, dispatching
    /// on its [`ProposalAction`] variant.
    ///
    /// # Target implementation
    /// Tracked as a **security-sensitive** open contributor issue. It
    /// must, in order:
    /// 1. Load the [`crate::types::Proposal`] by `proposal_id` (else
    ///    [`VaultError::ProposalNotFound`]).
    /// 2. Confirm `status == Ready` (else
    ///    [`VaultError::ProposalNotReady`]).
    /// 3. Confirm `env.ledger().sequence() >= proposal.executable_after_ledger`
    ///    (else [`VaultError::TimelockNotExpired`]).
    /// 4. Confirm `proposal.approvals.len() >= config.threshold` as a
    ///    defense-in-depth re-check (the `Ready` transition in
    ///    [`VaultFactory::approve`] should already guarantee this, but
    ///    execution is the last line of defense before funds move).
    /// 5. Dispatch on `proposal.action`:
    ///    - `Transfer { asset, to, amount }`: if a [`SpendingLimit`] is
    ///      configured for `asset`, load/roll over the
    ///      [`crate::types::SpendingUsage`] window (via
    ///      `env.ledger().sequence()` vs
    ///      `SpendingUsage::period_start_ledger` +
    ///      `SpendingLimit::period_ledgers`) and reject with
    ///      [`VaultError::SpendingLimitExceeded`] if `spent + amount`
    ///      would exceed `limit_per_period`; otherwise record the usage
    ///      and invoke the token contract's `transfer`.
    ///    - `GenericInvoke { contract, function, args }`: invoke via
    ///      `env.invoke_contract`.
    ///    - `UpdateSigners { signers, threshold }`: re-validate via the
    ///      same rules as [`VaultFactory::initialize`] and overwrite
    ///      [`VaultConfig`] in instance storage.
    /// 6. Transition `proposal.status` to `Executed` and persist.
    ///
    /// # Errors
    /// - [`VaultError::NotInitialized`]
    /// - [`VaultError::ProposalNotFound`]
    /// - [`VaultError::ProposalNotReady`]
    /// - [`VaultError::TimelockNotExpired`]
    /// - [`VaultError::InsufficientApprovals`]
    /// - [`VaultError::SpendingLimitNotConfigured`] /
    ///   [`VaultError::SpendingLimitExceeded`]
    /// - [`VaultError::ArithmeticError`]
    pub fn execute(env: Env, executor: Address, proposal_id: u64) -> Result<(), VaultError> {
        let _ = (&env, &executor, proposal_id);
        todo!("Timelock + spending-limit enforcement + action dispatch — see doc-comment for the exact required steps")
    }

    /// Deploys a new, independent vault contract instance (a fresh
    /// `VaultConfig`, its own signer set/threshold/timelock, and its own
    /// address) using this contract as the factory.
    ///
    /// # Target implementation
    /// Tracked as an open contributor issue covering Soroban's
    /// deployer/executable framework. It must:
    /// 1. Use `env.deployer().with_current_contract(salt)` (or
    ///    `with_address` if deploying on behalf of a different deployer
    ///    identity) to deterministically derive the new contract's
    ///    address from `wasm_hash` and `salt`.
    /// 2. Deploy the child contract via `.deploy(wasm_hash)`, obtaining
    ///    its `Address`.
    /// 3. Invoke the new contract's own `initialize` (via
    ///    `env.invoke_contract`) with `signers`, `threshold`, and
    ///    `timelock_blocks`, propagating any failure as
    ///    [`VaultError::DeploymentFailed`] rather than leaving a
    ///    half-initialized vault reachable.
    /// 4. Return the new vault's `Address` to the caller (e.g. so an
    ///    off-chain indexer or UI can immediately point at it).
    ///
    /// Contributors should also decide and document whether
    /// `deploy_vault` itself should be permissioned (e.g. restricted to an
    /// admin `Address` stored at factory-initialization time) or left
    /// permissionless, since anyone deploying a vault only spends their
    /// own resource fees and cannot affect existing vaults.
    ///
    /// # Errors
    /// - [`VaultError::DeploymentFailed`]
    pub fn deploy_vault(
        env: Env,
        wasm_hash: BytesN<32>,
        salt: BytesN<32>,
        signers: Vec<Address>,
        threshold: u32,
        timelock_blocks: u32,
    ) -> Result<Address, VaultError> {
        let _ = (
            &env,
            &wasm_hash,
            &salt,
            &signers,
            threshold,
            timelock_blocks,
        );
        todo!("Deterministic child-contract deployment via env.deployer() — see doc-comment for the exact required steps")
    }
}

/// Shared validation for a candidate signer set: non-empty, no duplicates,
/// within [`MAX_SIGNERS`]. Used by both `initialize` and (once
/// implemented) the `UpdateSigners` branch of [`VaultFactory::execute`].
fn validate_signer_set(signers: &Vec<Address>) -> Result<(), VaultError> {
    if signers.is_empty() {
        return Err(VaultError::EmptySignerSet);
    }
    if signers.len() > MAX_SIGNERS {
        return Err(VaultError::InvalidThreshold);
    }
    for i in 0..signers.len() {
        for j in (i + 1)..signers.len() {
            if signers.get_unchecked(i) == signers.get_unchecked(j) {
                return Err(VaultError::DuplicateSigner);
            }
        }
    }
    Ok(())
}
