# soroban-VaultFactory

[![CI](https://github.com/Ibawa12/soroban_vaultFactory_crates/actions/workflows/ci.yml/badge.svg)](https://github.com/Ibawa12/soroban_vaultFactory_crates/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A multi-signature, timelocked vault contract for [Soroban](https://developers.stellar.org/docs/build/smart-contracts/overview), Stellar's smart contract platform. It combines three things treasuries actually need — M-of-N signer approval, a mandatory delay before funds move, and per-asset spending limits — into one contract built on Soroban's native `auth` framework.

## What problem this solves

Any team, DAO, or protocol that wants to hold funds on Stellar has to answer the same question first: who can move this money, and under what conditions? Today there's no shared answer in the Soroban ecosystem — teams either use a single signer (a single point of failure), roll their own multisig contract from scratch (reinventing the same security-critical logic every time), or try to port over a pattern from another chain that doesn't map cleanly onto Soroban's auth model, storage TTLs, or resource metering.

A signature threshold alone isn't enough either. Without a timelock, a compromised or colluding majority of signers can drain a treasury before anyone reacts. Without spending limits, every payout — no matter how routine — needs the same full governance process as a treasury-emptying transfer.

This contract exists so that "multisig + timelock + spending limits, done correctly on Soroban's own primitives" is something you can depend on instead of rebuild.

## Running it

You'll need the `wasm32v1-none` target (pinned in `rust-toolchain.toml`) — not `wasm32-unknown-unknown`, which produces a Wasm encoding Soroban's host currently rejects at upload time.

```bash
rustup target add wasm32v1-none
cargo build --target wasm32v1-none --release   # build the deployable contract wasm
cargo test                                     # run the test suite
```

Once built, deploy and interact with it like any other Soroban contract via the [Soroban CLI](https://developers.stellar.org/docs/tools/developer-tools#cli):

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/soroban_VaultFactory.wasm \
  --source <your-account> --network testnet
```

A minimal usage sketch:

```rust
// Stand up a 2-of-3 vault with a ~5.5-day timelock
let signers = vec![&env, alice.clone(), bob.clone(), carol.clone()];
client.initialize(&signers, &2u32, &100_000u32);

// Propose, approve, and (after the timelock) execute a transfer
let action = ProposalAction::Transfer(TransferAction {
    asset: usdc_token,
    to: recipient,
    amount: 5_000_0000000,
});
let proposal_id = client.propose(&alice, &action);
client.approve(&bob, &proposal_id);
client.execute(&alice, &proposal_id);
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup, coding conventions, and PR checklist.

## Security

Please don't open a public issue for suspected vulnerabilities — see [SECURITY.md](SECURITY.md) for the private reporting process.

## License

Apache-2.0 — see [LICENSE](LICENSE).

## Code of Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
