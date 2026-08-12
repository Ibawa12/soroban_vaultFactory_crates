//! Main contract entrypoints: initialization, spending-limit
//! configuration, the full proposal lifecycle (propose/approve/execute),
//! and factory-style deployment of new vault instances.
//!
//! All six entrypoints follow the same pattern: auth check -> validate ->
//! load/mutate state via [`crate::storage`] -> persist.

use soroban_sdk::token::TokenClient;
use soroban_sdk::xdr::FromXdr;
use soroban_sdk::{
    contract, contractimpl, Address, BytesN, Env, IntoVal, MuxedAddress, Symbol, Val, Vec,
};

use crate::errors::VaultError;
use crate::storage;
use crate::types::{
    ProposalAction, ProposalStatus, SpendingLimit, SpendingUsage, VaultConfig, MAX_SIGNERS,
    MAX_TIMELOCK_LEDGERS,
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
    /// Execution itself is intentionally permissionless: `executor` is not
    /// required to be a current signer and is not auth-checked. The
    /// multisig/timelock gate on the *proposal* is what authorizes the
    /// underlying action; requiring a specific caller to also submit the
    /// execution transaction would only add friction (e.g. a "keeper" or
    /// off-chain relayer submitting on a signer's behalf) without adding
    /// security, since `execute` re-validates `Ready` status, the timelock,
    /// and the approval count itself before doing anything irreversible.
    ///
    /// # Errors
    /// - [`VaultError::NotInitialized`]
    /// - [`VaultError::ProposalNotFound`]
    /// - [`VaultError::ProposalNotReady`]
    /// - [`VaultError::TimelockNotExpired`]
    /// - [`VaultError::InsufficientApprovals`]
    /// - [`VaultError::SpendingLimitExceeded`]
    /// - [`VaultError::ArithmeticError`]
    /// - [`VaultError::InvalidActionPayload`] (a `GenericInvoke` argument
    ///   fails to decode back into a `Val`)
    /// - [`VaultError::EmptySignerSet`], [`VaultError::DuplicateSigner`],
    ///   [`VaultError::InvalidThreshold`] (an `UpdateSigners` action whose
    ///   payload fails the same validation `initialize` applies)
    pub fn execute(env: Env, executor: Address, proposal_id: u64) -> Result<(), VaultError> {
        let _ = &executor;
        let config = storage::get_config(&env)?;
        let mut proposal = storage::get_proposal(&env, proposal_id)?;

        if proposal.status != ProposalStatus::Ready {
            return Err(VaultError::ProposalNotReady);
        }
        if env.ledger().sequence() < proposal.executable_after_ledger {
            return Err(VaultError::TimelockNotExpired);
        }
        if proposal.approvals.len() < config.threshold {
            return Err(VaultError::InsufficientApprovals);
        }

        match &proposal.action {
            ProposalAction::Transfer(transfer) => {
                if let Some(limit) = storage::get_spending_limit(&env, &transfer.asset) {
                    let current_ledger = env.ledger().sequence();
                    let mut usage = storage::get_spending_usage(&env, &transfer.asset)
                        .filter(|u| current_ledger < u.period_start_ledger + limit.period_ledgers)
                        .unwrap_or(SpendingUsage {
                            spent: 0,
                            period_start_ledger: current_ledger,
                        });
                    let new_spent = usage
                        .spent
                        .checked_add(transfer.amount)
                        .ok_or(VaultError::ArithmeticError)?;
                    if new_spent > limit.limit_per_period {
                        return Err(VaultError::SpendingLimitExceeded);
                    }
                    usage.spent = new_spent;
                    storage::set_spending_usage(&env, &transfer.asset, &usage);
                }
                let token = TokenClient::new(&env, &transfer.asset);
                let to: MuxedAddress = transfer.to.clone().into();
                token.transfer(&env.current_contract_address(), &to, &transfer.amount);
            }
            ProposalAction::GenericInvoke(invoke) => {
                let mut args: Vec<Val> = Vec::new(&env);
                for arg_bytes in invoke.args.iter() {
                    let arg = Val::from_xdr(&env, &arg_bytes)
                        .map_err(|_| VaultError::InvalidActionPayload)?;
                    args.push_back(arg);
                }
                let _: Val = env.invoke_contract(&invoke.contract, &invoke.function, args);
            }
            ProposalAction::UpdateSigners(update) => {
                validate_signer_set(&update.signers)?;
                if update.threshold == 0 || update.threshold > update.signers.len() {
                    return Err(VaultError::InvalidThreshold);
                }
                let new_config = VaultConfig {
                    signers: update.signers.clone(),
                    threshold: update.threshold,
                    timelock_blocks: config.timelock_blocks,
                    created_at_ledger: config.created_at_ledger,
                };
                storage::set_config(&env, &new_config);
            }
        }

        proposal.status = ProposalStatus::Executed;
        storage::set_proposal(&env, &proposal);

        Ok(())
    }

    /// Deploys a new, independent vault contract instance (a fresh
    /// `VaultConfig`, its own signer set/threshold/timelock, and its own
    /// address) using this contract as the factory.
    ///
    /// Deliberately left **permissionless**: anyone may deploy a new vault
    /// through this factory, since doing so only spends the caller's own
    /// resource fees, derives an address the caller (via `salt`) already
    /// controls the derivation of, and cannot read or affect any other
    /// vault's state — there is nothing here for permissioning to protect.
    /// A future admin-gated variant, if wanted, is additive and doesn't
    /// need to replace this one.
    ///
    /// # Errors
    /// - [`VaultError::DeploymentFailed`] if the new instance's own
    ///   `initialize` call fails for any reason: invalid `signers`/
    ///   `threshold`/`timelock_blocks`, or the deployed Wasm not exposing
    ///   an `initialize(signers, threshold, timelock_blocks)` entrypoint
    ///   at all — e.g. because `wasm_hash` doesn't point at this same
    ///   `VaultFactory` contract. This deliberately collapses every
    ///   failure mode into one variant rather than trying to propagate the
    ///   child's own [`VaultError`] discriminant, since `wasm_hash` is
    ///   caller-supplied and nothing guarantees the deployed contract
    ///   shares this crate's error taxonomy.
    pub fn deploy_vault(
        env: Env,
        wasm_hash: BytesN<32>,
        salt: BytesN<32>,
        signers: Vec<Address>,
        threshold: u32,
        timelock_blocks: u32,
    ) -> Result<Address, VaultError> {
        let deployer = env.deployer().with_current_contract(salt);
        let vault_address = deployer.deploy_v2(wasm_hash, ());

        let mut args: Vec<Val> = Vec::new(&env);
        args.push_back(signers.into_val(&env));
        args.push_back(threshold.into_val(&env));
        args.push_back(timelock_blocks.into_val(&env));

        let init_fn = Symbol::new(&env, "initialize");
        match env.try_invoke_contract::<(), VaultError>(&vault_address, &init_fn, args) {
            Ok(Ok(())) => Ok(vault_address),
            _ => Err(VaultError::DeploymentFailed),
        }
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
