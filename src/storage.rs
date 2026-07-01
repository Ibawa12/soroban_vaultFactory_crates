//! Storage layer: explicit key wrappers and typed accessors, strictly
//! separated by Soroban storage class.
//!
//! - **Instance** storage holds the vault's long-term configuration
//!   ([`VaultConfig`], the proposal-id counter, and per-asset
//!   [`SpendingLimit`]s) — data with no natural expiry, whose TTL should be
//!   extended in lockstep with the contract instance itself.
//! - **Temporary** storage holds ephemeral, self-expiring state:
//!   in-flight [`Proposal`]s and per-asset [`SpendingUsage`] counters.
//!   Both are naturally period-scoped (a proposal resolves or the vault
//!   stops caring about it; a spending period rolls over) so letting the
//!   host evict them once their TTL lapses is the correct default, and
//!   cheaper than the contract explicitly clearing them.
//!
//! [`InstanceDataKey`] and [`TemporaryDataKey`] are two separate enums
//! (rather than one shared key enum) specifically so that a key value can
//! never be constructed that accidentally collides across storage classes
//! or is written to the wrong one by copy-paste error — the type system
//! forces the caller to pick a storage class at the call site.

use soroban_sdk::{contracttype, Address, Env};

use crate::errors::VaultError;
use crate::types::{Proposal, SpendingLimit, SpendingUsage, VaultConfig};

/// Extend an instance entry's TTL to at least this many ledgers whenever
/// it is read, if fewer than [`INSTANCE_BUMP_THRESHOLD`] ledgers remain.
/// ~30 days at ~5s/ledger.
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17 * 24 * 60 * 60 / 5;
/// Target TTL (in ledgers) instance entries are extended to on bump.
/// ~60 days at ~5s/ledger.
pub const INSTANCE_BUMP_TO: u32 = 34 * 24 * 60 * 60 / 5;

/// Extend a temporary entry's TTL if fewer than this many ledgers remain.
/// ~3 days at ~5s/ledger — proposals and spending-usage windows are
/// expected to resolve on this order of magnitude.
pub const TEMPORARY_BUMP_THRESHOLD: u32 = 3 * 24 * 60 * 60 / 5;
/// Target TTL (in ledgers) temporary entries are extended to on bump.
/// ~7 days at ~5s/ledger.
pub const TEMPORARY_BUMP_TO: u32 = 7 * 24 * 60 * 60 / 5;

/// Keys for values held in **instance** storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstanceDataKey {
    /// -> [`VaultConfig`]
    Config,
    /// -> `u64`, the next id to assign to a newly created [`Proposal`].
    ProposalCounter,
    /// -> [`SpendingLimit`], keyed by the asset (token contract address)
    /// it constrains.
    SpendingLimit(Address),
}

/// Keys for values held in **temporary** storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemporaryDataKey {
    /// -> [`Proposal`], keyed by [`Proposal::id`].
    Proposal(u64),
    /// -> [`SpendingUsage`], keyed by the asset (token contract address)
    /// it accumulates usage for.
    SpendingUsage(Address),
}

/// Returns `true` if `initialize` has already run on this contract
/// instance.
pub fn has_config(env: &Env) -> bool {
    env.storage().instance().has(&InstanceDataKey::Config)
}

/// Loads the vault's [`VaultConfig`], bumping the instance TTL on read.
///
/// # Errors
/// [`VaultError::NotInitialized`] if `initialize` has not yet run.
pub fn get_config(env: &Env) -> Result<VaultConfig, VaultError> {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&InstanceDataKey::Config)
        .ok_or(VaultError::NotInitialized)
}

/// Persists `config` to instance storage, overwriting any prior value.
pub fn set_config(env: &Env, config: &VaultConfig) {
    env.storage().instance().set(&InstanceDataKey::Config, config);
    bump_instance(env);
}

/// Atomically allocates and returns the next [`Proposal`] id, persisting
/// the incremented counter back to instance storage.
pub fn next_proposal_id(env: &Env) -> u64 {
    let key = InstanceDataKey::ProposalCounter;
    let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
    let next = current + 1;
    env.storage().instance().set(&key, &next);
    bump_instance(env);
    current
}

/// Loads the [`SpendingLimit`] configured for `asset`, if any.
pub fn get_spending_limit(env: &Env, asset: &Address) -> Option<SpendingLimit> {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&InstanceDataKey::SpendingLimit(asset.clone()))
}

/// Persists a [`SpendingLimit`] for `asset`, overwriting any prior value.
pub fn set_spending_limit(env: &Env, asset: &Address, limit: &SpendingLimit) {
    env.storage()
        .instance()
        .set(&InstanceDataKey::SpendingLimit(asset.clone()), limit);
    bump_instance(env);
}

/// Removes any [`SpendingLimit`] configured for `asset`.
pub fn remove_spending_limit(env: &Env, asset: &Address) {
    env.storage()
        .instance()
        .remove(&InstanceDataKey::SpendingLimit(asset.clone()));
}

/// Extends the contract instance's own TTL (and therefore every key held
/// in instance storage) if it is due for a bump. Cheap to call
/// unconditionally — `extend_ttl` is a no-op when the current TTL already
/// exceeds `to`.
pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TO);
}

/// Loads the [`Proposal`] with the given `id` from temporary storage,
/// bumping its TTL on read.
///
/// # Errors
/// [`VaultError::ProposalNotFound`] if no such proposal exists — either it
/// was never created, or its temporary-storage TTL has already lapsed and
/// the host evicted it.
pub fn get_proposal(env: &Env, id: u64) -> Result<Proposal, VaultError> {
    let key = TemporaryDataKey::Proposal(id);
    let proposal = env
        .storage()
        .temporary()
        .get(&key)
        .ok_or(VaultError::ProposalNotFound)?;
    env.storage()
        .temporary()
        .extend_ttl(&key, TEMPORARY_BUMP_THRESHOLD, TEMPORARY_BUMP_TO);
    Ok(proposal)
}

/// Persists `proposal` to temporary storage under `proposal.id`,
/// overwriting any prior value, and (re-)sets its TTL.
pub fn set_proposal(env: &Env, proposal: &Proposal) {
    let key = TemporaryDataKey::Proposal(proposal.id);
    env.storage().temporary().set(&key, proposal);
    env.storage()
        .temporary()
        .extend_ttl(&key, TEMPORARY_BUMP_THRESHOLD, TEMPORARY_BUMP_TO);
}

/// Removes a [`Proposal`] from temporary storage (e.g. once executed or
/// cancelled, if the caller wants to reclaim the slot rather than let it
/// expire naturally).
pub fn remove_proposal(env: &Env, id: u64) {
    env.storage().temporary().remove(&TemporaryDataKey::Proposal(id));
}

/// Loads the current [`SpendingUsage`] window for `asset`, if the host
/// still has one live in temporary storage (a lapsed/evicted entry is
/// indistinguishable from "no spending yet this period" and callers should
/// treat `None` as a fresh window).
pub fn get_spending_usage(env: &Env, asset: &Address) -> Option<SpendingUsage> {
    let key = TemporaryDataKey::SpendingUsage(asset.clone());
    let usage = env.storage().temporary().get(&key)?;
    env.storage()
        .temporary()
        .extend_ttl(&key, TEMPORARY_BUMP_THRESHOLD, TEMPORARY_BUMP_TO);
    Some(usage)
}

/// Persists `usage` to temporary storage for `asset`, overwriting any
/// prior value, and (re-)sets its TTL.
pub fn set_spending_usage(env: &Env, asset: &Address, usage: &SpendingUsage) {
    let key = TemporaryDataKey::SpendingUsage(asset.clone());
    env.storage().temporary().set(&key, usage);
    env.storage()
        .temporary()
        .extend_ttl(&key, TEMPORARY_BUMP_THRESHOLD, TEMPORARY_BUMP_TO);
}
