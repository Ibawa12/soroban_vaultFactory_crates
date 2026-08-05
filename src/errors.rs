//! Error taxonomy for `vault-factory`.
//!
//! Every state-mutating entrypoint returns/panics through one of these
//! variants (never a bare `panic!`), so integrators and off-chain indexers
//! get a stable, documented error code rather than an opaque host trap.
//! Discriminants are part of the on-chain ABI and must never be reordered
//! or reused once shipped.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    // --- Lifecycle -----------------------------------------------------
    /// `initialize` was called on a contract instance that already has a
    /// [`crate::types::VaultConfig`] in instance storage.
    AlreadyInitialized = 1,
    /// Any entrypoint other than `initialize` was called before the vault
    /// had a [`crate::types::VaultConfig`] in instance storage.
    NotInitialized = 2,

    // --- Signer / threshold configuration -------------------------------
    /// The signer list passed to `initialize` or an `UpdateSigners`
    /// proposal was empty.
    EmptySignerSet = 10,
    /// The signer list contained the same `Address` more than once.
    DuplicateSigner = 11,
    /// `threshold` was zero, or greater than the number of signers
    /// (an M-of-N vault can never require more approvals than it has
    /// possible approvers).
    InvalidThreshold = 12,
    /// An address expected to be a current signer of the vault was not
    /// found in [`crate::types::VaultConfig::signers`].
    SignerNotFound = 13,

    // --- Timelock --------------------------------------------------------
    /// `timelock_blocks` supplied at vault creation exceeded
    /// [`crate::types::MAX_TIMELOCK_LEDGERS`] or was configured
    /// inconsistently (e.g. implied a negative delay).
    InvalidTimelockDuration = 20,
    /// A proposal was submitted for execution before its
    /// `executable_after_ledger` was reached.
    TimelockNotExpired = 21,

    // --- Proposal lifecycle ----------------------------------------------
    /// No [`crate::types::Proposal`] exists under the requested id in
    /// temporary storage (it may never have existed, or it may have
    /// already expired and been evicted by the host).
    ProposalNotFound = 30,
    /// The proposal's `status` was not [`crate::types::ProposalStatus::Pending`]
    /// when an approval or cancellation was attempted.
    ProposalNotPending = 31,
    /// The proposal's `status` was not [`crate::types::ProposalStatus::Ready`]
    /// when execution was attempted.
    ProposalNotReady = 32,
    /// A signer attempted to approve a proposal they had already approved.
    DuplicateApproval = 33,
    /// The proposal did not yet have `threshold` distinct signer approvals
    /// recorded when execution was attempted.
    InsufficientApprovals = 34,

    // --- Spending limits ---------------------------------------------------
    /// No [`crate::types::SpendingLimit`] is configured in instance storage
    /// for the requested asset, but the proposed action requires one.
    SpendingLimitNotConfigured = 40,
    /// Executing the proposal's transfer would exceed the configured
    /// per-period [`crate::types::SpendingLimit`] for that asset.
    SpendingLimitExceeded = 41,

    // --- Auth / factory ------------------------------------------------
    /// `require_auth` (or the aggregated M-of-N verification loop) failed
    /// for the calling address.
    Unauthorized = 50,
    /// The factory's `deploy_vault` call failed to install or initialize
    /// the child contract instance (see
    /// [`crate::contract::VaultFactory::deploy_vault`] for the exact
    /// failure points this can wrap once implemented).
    DeploymentFailed = 51,

    // --- Arithmetic --------------------------------------------------------
    /// A spending-limit or amount calculation overflowed/underflowed
    /// `i128`, or divided by zero.
    ArithmeticError = 60,

    // --- Action payload ------------------------------------------------
    /// A [`crate::types::GenericInvokeAction`] argument's XDR-encoded
    /// bytes (`GenericInvokeAction::args`) failed to decode back into a
    /// `Val` at execution time — either it was never validly-encoded XDR
    /// in the first place, or it encodes an `ScVal` shape this host
    /// doesn't accept as an invocation argument.
    InvalidActionPayload = 70,
}
