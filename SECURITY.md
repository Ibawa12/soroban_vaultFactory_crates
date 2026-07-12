# Security Policy

`soroban-VaultFactory` is designed to hold and move real funds under
multisig, timelock, and spending-limit guarantees. A vulnerability here has
direct financial impact. Please report suspected vulnerabilities privately
rather than as a public issue.

## Supported versions

This project has not yet had a tagged `1.0` release. Until then, only the
latest commit on the `master` branch is supported — there are no older
version branches receiving security fixes. Once tagged releases begin, this
section will be updated with a version support table.

| Version | Supported |
|---|---|
| `master` (latest commit) | ✅ |
| Anything else | ❌ |

## Project status: pre-audit, not production-ready

This contract has **not** undergone an external security audit, and its
most security-critical entrypoints — `approve` (the M-of-N verification
loop), `execute` (the entrypoint that actually moves funds), and
`deploy_vault` — are currently unimplemented `todo!()` stubs (see the
[README](README.md#status)). **Do not deploy this contract to hold real
funds** until it has a complete implementation and an independent audit.
If you're evaluating it for that purpose, please open a discussion issue
first — we'd genuinely like to know before it happens, not after.

## Reporting a vulnerability

**Please do not open a public GitHub issue for a suspected vulnerability**,
including (especially) anything that could allow funds to move without
proper authorization, bypass the timelock, exceed a configured spending
limit, or lock funds permanently.

Use GitHub's private vulnerability reporting instead:

1. Go to the [Security tab](../../security) of this repository.
2. Click **"Report a vulnerability"** to open a private advisory draft.
3. Include as much of the following as you can:
   - The affected entrypoint(s) and file(s)
   - A minimal reproduction: the vault configuration (signers, threshold,
     timelock), the sequence of calls, and the unexpected outcome
   - Your assessment of impact — e.g. "unauthorized fund movement,"
     "timelock bypass," "spending-limit bypass," "funds permanently
     locked," or "denial of service on a specific entrypoint"
   - Whether exploitation requires a malicious/colluding signer, or is
     reachable by a completely unauthorized caller

If private vulnerability reporting isn't available or accessible to you,
open an issue titled only `Security contact needed` with no technical
details, and a maintainer will follow up with a private channel.

## What counts as in-scope

- Any sequence of calls that moves funds without the configured M-of-N
  threshold of signer approvals
- Any sequence of calls that executes a proposal before its timelock has
  elapsed
- Any sequence of calls that exceeds a configured `SpendingLimit` for an
  asset
- Any way to permanently lock funds in a vault that a correctly-behaving
  set of signers should be able to recover
- Reentrancy or cross-call ordering issues around `execute`'s external
  calls (see [issue: reentrancy audit](../../issues) once filed/tracked)
- Any panic reachable from a documented public API with a well-formed
  input (as opposed to a documented `todo!()` in an intentionally
  unimplemented function — those are tracked as open issues, not
  vulnerabilities)

## What's out of scope

- The known-unimplemented `todo!()` bodies in `src/contract.rs`
  (`approve`, `execute`, `deploy_vault`) — these are tracked as
  [open issues](../../issues), not vulnerabilities, until an
  implementation lands
- Attacks that require control of a majority of a vault's own configured
  signers acting maliciously in a way the vault's threshold was
  explicitly configured to tolerate (e.g. a 2-of-3 vault where 2 signers
  are compromised is a key-management problem, not a contract bug) —
  though we do want to hear about anything that makes this *worse* than
  the vault's configured threshold implies
- Findings against `GenericInvoke` calls into third-party contracts that
  are themselves malicious or buggy — this vault executes what it's
  told to execute once authorized; it isn't responsible for the safety
  of arbitrary third-party contract code a proposal points at

## Response process

We aim to acknowledge a new report within **3 business days** given the
financial-impact nature of this project, and to provide an initial
assessment (confirmed / not a vulnerability / needs more information)
within **7 business days**. Given the project's current volunteer-
maintained, pre-1.0 stage, please treat these as targets rather than
guarantees — if you haven't heard back in that window, following up on
the same advisory thread is welcome and expected.

We follow coordinated disclosure: please give us a reasonable window to
land and release a fix before any public disclosure. We'll credit
reporters (by name or handle, or anonymously if you prefer) in the fix's
release notes unless you ask us not to.
