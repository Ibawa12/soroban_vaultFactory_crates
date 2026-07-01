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

/// Maximum number of signers a single vault may configure. Bounded so that
/// the M-of-N approval loop (a linear scan over `signers` per proposal
/// approval) stays within predictable gas/CPU-instruction budgets.
pub const MAX_SIGNERS: u32 = 20;

/// The durable, instance-storage configuration of a single vault: its
/// signer set, approval threshold, and default timelock delay.
///
/// This is written once by `initialize` and thereafter only mutated by a
/// successfully executed [`ProposalAction::UpdateSigners`] proposal — never
/// by a direct, unauthenticated setter.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConfig {
    /// The current set of addresses authorized to approve proposals.
    /// Order is insertion order and carries no privilege semantics.
    pub signers: Vec<Address>,
    /// Number of distinct signer approvals required before a proposal
    /// transitions from `Pending` to `Ready` (the "M" in M-of-N).
    pub threshold: u32,
    /// Default number of ledgers a newly created proposal must wait
    /// between reaching `threshold` approvals and becoming executable.
    /// Individual proposals snapshot this into their own
    /// `executable_after_ledger` at creation time, so a later config
    /// change never retroactively affects an in-flight proposal.
    pub timelock_blocks: u32,
    /// Ledger sequence number at which this vault was initialized.
    pub created_at_ledger: u32,
}

/// A per-asset, per-period spending ceiling enforced independently of the
/// multisig/timelock flow — e.g. to let a vault permit small, frequent
/// transfers without a full proposal round-trip in a future fast-path, or
/// simply to cap the blast radius of any single approved proposal.
///
/// Stored in **instance** storage (it is long-lived configuration), while
/// the *usage* counter tracking consumption against this limit
/// ([`SpendingUsage`]) lives in **temporary** storage since it is an
/// ephemeral, period-scoped accumulator that should naturally expire.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpendingLimit {
    /// The token contract address this limit applies to.
    pub asset: Address,
    /// Maximum total amount (in the asset's native, unscaled integer
    /// units — the same units `token::Client::transfer` expects) that may
    /// be transferred out within any single rolling window of
    /// `period_ledgers`.
    pub limit_per_period: i128,
    /// Length, in ledgers, of the rolling window `limit_per_period`
    /// applies to.
    pub period_ledgers: u32,
}

/// Ephemeral usage counter tracked against a [`SpendingLimit`]. Lives in
/// **temporary** storage keyed by asset (see
/// [`crate::storage::TemporaryDataKey::SpendingUsage`]) so that once a
/// period lapses and no entrypoint touches it, the host garbage-collects
/// the entry automatically instead of the contract needing to explicitly
/// reset it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpendingUsage {
    /// Cumulative amount transferred so far within the current period.
    pub spent: i128,
    /// Ledger sequence number the current rolling window started at.
    pub period_start_ledger: u32,
}

/// Payload for [`ProposalAction::Transfer`]. Broken out into its own
/// `#[contracttype]` struct because Soroban's contract-type derive only
/// supports enum variants that are either unit variants or a single
/// tuple field — not multi-field struct variants — so each
/// [`ProposalAction`] case wraps one of these instead.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferAction {
    /// The token contract address to transfer from the vault.
    pub asset: Address,
    /// The recipient.
    pub to: Address,
    /// Amount to transfer, in the asset's native, unscaled integer units.
    pub amount: i128,
}

/// Payload for [`ProposalAction::GenericInvoke`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericInvokeAction {
    pub contract: Address,
    pub function: Symbol,
    /// Each argument as its individual XDR-encoded `ScVal` bytes (e.g. via
    /// `arg_val.to_xdr(&env)` at proposal-creation time), rather than
    /// `soroban_sdk::Val` directly. A `Val` is a lightweight handle into
    /// the *current* host frame and is not meaningful once that
    /// invocation returns, so it cannot be written to persistent or
    /// temporary storage — a `Proposal` submitted in one transaction and
    /// executed in a later one would hold dangling handles. Whatever
    /// eventually implements [`crate::contract::VaultFactory::execute`]
    /// must decode each entry back into a `Val` (via
    /// `Val::from_xdr(&env, bytes)` or the equivalent typed conversion)
    /// immediately before dispatching the cross-contract call.
    pub args: Vec<Bytes>,
}

/// Payload for [`ProposalAction::UpdateSigners`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateSignersAction {
    pub signers: Vec<Address>,
    pub threshold: u32,
}

/// The concrete effect a [`Proposal`] will have if it reaches `Ready` and
/// is executed. Modeled as an enum (rather than always shelling out to a
/// generic cross-contract call) so that the common, security-sensitive
/// "move funds" case can be validated against [`SpendingLimit`]s directly
/// by the contract, without needing to parse arbitrary `args`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalAction {
    /// Transfer `amount` of `asset` to `to` via that asset's
    /// `token::Client::transfer`. Subject to [`SpendingLimit`] checks if
    /// one is configured for `asset`.
    Transfer(TransferAction),
    /// Arbitrary cross-contract invocation, for vault use cases beyond
    /// simple transfers (e.g. voting in a DAO, staking, claiming rewards).
    /// Not subject to spending-limit checks — vaults that want to restrict
    /// this should omit signers who don't need it, or rely on the
    /// threshold/timelock alone.
    GenericInvoke(GenericInvokeAction),
    /// Replace the vault's signer set and/or threshold. Executing this
    /// action overwrites [`VaultConfig::signers`] and
    /// [`VaultConfig::threshold`] in instance storage.
    UpdateSigners(UpdateSignersAction),
}

