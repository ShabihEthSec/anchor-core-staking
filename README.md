# anchor-core-staking

> **Turbin3 Q2 Builders Cohort — Week 5 Assignment**
> NFT Staking on Solana using Metaplex Core (MPL Core) + Anchor

A fully on-chain NFT staking program built with the [Anchor framework](https://www.anchor-lang.com/) and [Metaplex Core](https://developers.metaplex.com/core). NFTs are frozen in-place using the **Freeze Delegate** plugin and all staking state is stored directly on the asset via the **Attributes** plugin — no traditional PDA vaults required. The program supports independent reward claiming, clean unstaking, and collection-level staking counters.

---

## Table of Contents

- [Features](#features)
- [Challenges Completed](#challenges-completed)
- [Architecture](#architecture)
- [Program Instructions](#program-instructions)
- [Account Structure](#account-structure)
- [Getting Started](#getting-started)
- [Running Tests](#running-tests)
- [Project Structure](#project-structure)
- [Tech Stack](#tech-stack)

---

## Features

- **Stake NFT** — Freezes a Core NFT using the Freeze Delegate plugin and records staking start time + reward debt directly on the asset's Attributes plugin
- **Claim Rewards** — Allows a staker to harvest accrued rewards _without_ unstaking the NFT; updates the `last_claimed` attribute to prevent double-claiming
- **Unstake NFT** — Thaws the NFT, transfers any remaining unclaimed rewards, and removes staking state
- **Collection Attributes** — Tracks the total number of NFTs currently staked across the collection using an `Attributes` plugin added to the collection asset
- **Rewards Mint** — A PDA-controlled SPL token mint distributes rewards on a per-slot accrual basis

---

## Challenges Completed

### ✅ Challenge 1 — Separate `claim_rewards` Instruction

The program implements `claim_rewards` as a **standalone instruction**, fully independent from `unstake`. This satisfies both sub-requirements:

**a) Claim rewards without unstaking the NFT**
The `claim_rewards` instruction reads the staking start time and `last_claimed` timestamp stored in the asset's Attributes plugin, computes the accrued reward, mints tokens to the user's reward ATA, and writes the updated `last_claimed` value back to the asset — all while the NFT remains frozen and staked.

**b) Unstake right after claiming rewards**
The `unstake` instruction independently reads the `last_claimed` attribute to calculate only the _remaining_ unclaimed rewards since the last claim. This means a user can call `claim_rewards` → `unstake` in sequence and receive exactly the right amount in each transaction without any double-payment or missed rewards.

### ✅ Challenge 2 — Collection `Attributes` Plugin for Staked Count

An `Attributes` plugin is added to the **collection** asset (not just individual NFTs) containing a `staked_count` key. The program:

- **Increments** `staked_count` on the collection when an NFT is staked
- **Decrements** `staked_count` on the collection when an NFT is unstaked

This gives protocols and UIs a trustless, on-chain source of truth for how many NFTs from a collection are currently staked, without needing off-chain indexers.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    User Wallet                      │
└────────────────┬────────────────────────────────────┘
                 │
     ┌───────────▼───────────┐
     │    Anchor Program     │
     │  anchor-core-staking  │
     └───┬───────┬───────┬───┘
         │       │       │
    stake  claim  unstake
         │       │       │
         ▼       ▼       ▼
  ┌─────────────────────────────────┐
  │         MPL Core Asset          │
  │  Attributes Plugin (per NFT):   │
  │   - staked_at: i64              │
  │   - last_claimed: i64           │
  │  Freeze Delegate Plugin         │
  └─────────────────────────────────┘
         │
         ▼
  ┌─────────────────────────────────┐
  │       MPL Core Collection       │
  │  Attributes Plugin:             │
  │   - staked_count: u64           │
  └─────────────────────────────────┘
         │
         ▼
  ┌──────────────────────┐
  │  Rewards SPL Mint    │
  │  (PDA-controlled)    │
  └──────────────────────┘
```

Key design decision: staking state lives **on the asset itself** via the Attributes plugin, so there are no extra PDA storage accounts to rent-fund. The collection tracks aggregate staking state for on-chain composability.

---

## Program Instructions

### `initialize`

Sets up the `StakeConfig` PDA with configurable parameters:

- `points_per_stake` — reward tokens minted per slot per staked NFT
- `max_stake` — maximum NFTs a single user can stake
- `freeze_period` — minimum slots before unstaking is allowed

Also initializes the **rewards mint** as a PDA owned by the program, and adds the `Attributes` plugin to the collection with an initial `staked_count` of `0`.

### `stake`

1. Validates the NFT belongs to the expected collection
2. Adds / updates the `Attributes` plugin on the asset with `staked_at = clock.unix_timestamp` and `last_claimed = clock.unix_timestamp`
3. Adds the `Freeze Delegate` plugin and freezes the asset
4. Increments `staked_count` on the collection's Attributes plugin

### `claim_rewards`

1. Reads `last_claimed` from the asset's Attributes plugin
2. Calculates `reward = (now - last_claimed) * points_per_stake`
3. Mints reward tokens to the user's ATA
4. Updates `last_claimed = now` on the asset's Attributes plugin
5. **NFT stays frozen — no unstake occurs**

### `unstake`

1. Validates the freeze period has elapsed
2. Calculates remaining rewards since the last `claim_rewards` call (or since `staked_at` if never claimed)
3. Mints any remaining reward tokens
4. Thaws the asset and removes the Freeze Delegate plugin
5. Removes staking attributes from the asset
6. Decrements `staked_count` on the collection's Attributes plugin

---

## Account Structure

### `StakeConfig` (PDA: `["config", admin]`)

```rust
pub struct StakeConfig {
    pub points_per_stake: u8,
    pub max_stake: u8,
    pub freeze_period: u32,
    pub rewards_bump: u8,
    pub bump: u8,
}
```

### `UserAccount` (PDA: `["user", user_pubkey]`)

```rust
pub struct UserAccount {
    pub points: u32,
    pub amount_staked: u8,
    pub bump: u8,
}
```

Staking timestamps (`staked_at`, `last_claimed`) are stored directly on each MPL Core asset via the Attributes plugin, not in a separate PDA.

---

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (see `rust-toolchain.toml` for pinned version)
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools)
- [Anchor CLI](https://www.anchor-lang.com/docs/installation) `v0.31.x`
- [Node.js](https://nodejs.org/) `v18+` and `yarn`

### Install

```bash
git clone https://github.com/ShabihEthSec/anchor-core-staking.git
cd anchor-core-staking
yarn install
```

### Build

```bash
anchor build
```

### Deploy (localnet)

```bash
anchor localnet
# in a separate terminal:
anchor deploy
```

---

## Running Tests

Tests are written in TypeScript using `@metaplex-foundation/umi` for MPL Core interactions and cover all program instructions end-to-end.

```bash
anchor test
```

### Test Coverage

| Test                               | Description                                                                    |
| ---------------------------------- | ------------------------------------------------------------------------------ |
| `initialize config`                | Creates StakeConfig PDA and rewards mint; adds Attributes plugin to collection |
| `stake NFT`                        | Freezes asset, writes staking attributes, increments collection staked_count   |
| `claim rewards (mid-stake)`        | Claims accrued rewards without unstaking; verifies last_claimed update         |
| `unstake after claim`              | Unstakes after a prior claim; verifies only post-claim rewards are minted      |
| `unstake (no prior claim)`         | Full rewards paid on unstake when claim was never called                       |
| `collection staked_count tracking` | Verifies staked_count increments on stake and decrements on unstake            |
| `freeze period enforcement`        | Confirms unstake reverts if called before freeze_period elapses                |

All tests pass on a local validator. See the screenshot below.

---

## Project Structure

```
anchor-core-staking/
├── programs/
│   └── anchor-sore-staking/
│       └── src/
│           ├── lib.rs               # Program entry point, instruction dispatch
│           ├── instructions/
│           │   ├── initialize.rs    # StakeConfig + rewards mint + collection Attributes init
│           │   ├── stake.rs         # Freeze delegate + asset Attributes + collection counter
│           │   ├── claim_rewards.rs # Standalone reward claim (NFT stays staked)
│           │   └── unstake.rs       # Thaw + final reward mint + counter decrement
│           └── state/
│               ├── stake_config.rs  # StakeConfig account
│               └── user_account.rs  # UserAccount PDA
├── tests/
│   └── anchor-core-staking.ts       # Full TypeScript test suite
├── Anchor.toml
├── Cargo.toml
└── package.json
```

---

## Tech Stack

| Layer          | Tool                                                             |
| -------------- | ---------------------------------------------------------------- |
| Smart Contract | [Anchor](https://www.anchor-lang.com/) `v0.31.x`                 |
| NFT Standard   | [Metaplex Core (MPL Core)](https://developers.metaplex.com/core) |
| Token Standard | SPL Token (rewards mint)                                         |
| Test Client    | TypeScript + `@metaplex-foundation/umi`                          |
| Network        | Solana (localnet / devnet)                                       |
| Language       | Rust + TypeScript                                                |

---

## References

- [Metaplex Core Staking Example](https://github.com/metaplex-foundation/mpl-core-staking-example)
- [MPL Core Docs — Attributes Plugin](https://developers.metaplex.com/core/plugins/attribute)
- [MPL Core Docs — Freeze Delegate Plugin](https://developers.metaplex.com/core/plugins/freeze-delegate)
- [Anchor Documentation](https://www.anchor-lang.com/docs)
- Turbin3 Q2 Builders Cohort — Week 5 NFT Staking Video Resource

---

_Built by [@ShabihEthSec](https://github.com/ShabihEthSec) as part of the Turbin3 Q2 Builders Cohort._
