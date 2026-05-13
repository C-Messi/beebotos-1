---
name: binance
description: Use binance-cli for Binance Spot, Futures (USD-S), and Convert. Requires auth.
category: trading
tags: "[binance, crypto, trading, spot, futures, contract, convert]"
permissions:
  - process:exec
metadata:
  version: 1.1.0
  author: Binance
  openclaw:
    requires:
      bins:
        - binance-cli
    install:
      - kind: node
        package: '@binance/binance-cli'
        bins: [binance-cli]
        label: Install binance-cli (npm)
license: MIT
---

# Binance

Use `binance-cli` for Binance Spot, Futures (USD-S), and Convert. Requires auth.

> **PREREQUISITE:** Read [`auth.md`](./references/auth.md) for auth, global flags, and security rules.

## BeeBotOS Execution

BeeBotOS should use the `bash_shell` tool to execute `binance-cli` commands when the user asks to query Binance market/account data or operate Spot/Futures/Convert.

Run commands from this skill directory. Do not print API credentials. The active profile may already be configured in `binance-cli`; otherwise ask the user for the profile name and environment.

Before any real order, transfer, leverage/margin change, or other production transaction, ask the user to type `CONFIRM`. For Binance demo trading requests, still summarize the exact command and risk before execution.

Examples:

```bash
binance-cli profile view
binance-cli spot ticker-price --symbol BTCUSDT
binance-cli futures-usds symbol-price-ticker --symbol BTCUSDT
binance-cli futures-usds account-information-v3
```

## Helper Commands

| Command | Description |
|---------|-------------|
| [`algo`](./references/algo.md) | Algo Trading |
| [`alpha`](./references/alpha.md) | Alpha |
| [`c2c`](./references/c2c.md) | C2C |
| [`convert`](./references/convert.md) | Convert |
| [`copy-trading`](./references/copy-trading.md) | Copy Trading |
| [`crypto-loan`](./references/crypto-loan.md) | Crypto Loan |
| [`derivatives-options`](./references/derivatives-options.md) | Derivatives Trading (Options) |
| [`derivatives-portfolio-margin`](./references/derivatives-portfolio-margin.md) | Derivatives Trading (Portfolio Margin) |
| [`derivatives-portfolio-margin-pro`](./references/derivatives-portfolio-margin-pro.md) | Derivatives Trading (Portfolio Margin Pro) |
| [`dual-investment`](./references/dual-investment.md) | Dual Investment |
| [`fiat`](./references/fiat.md) | Fiat |
| [`futures-coin`](./references/futures-coin.md) | Derivatives Trading (COIN-M Futures) |
| [`futures-usds`](./references/futures-usds.md) | Derivatives Trading (USDS-M Futures) |
| [`gift-card`](./references/gift-card.md) | Gift Card |
| [`margin-trading`](./references/margin-trading.md) | Margin Trading |
| [`mining`](./references/mining.md) | Mining |
| [`pay`](./references/pay.md) | Pay |
| [`rebate`](./references/rebate.md) | Rebate |
| [`simple-earn`](./references/simple-earn.md) | Simple Earn |
| [`spot`](./references/spot.md) | Spot Trading |
| [`staking`](./references/staking.md) | Staking |
| [`sub-account`](./references/sub-account.md) | Sub Account |
| [`vip-loan`](./references/vip-loan.md) | VIP Loan |
| [`wallet`](./references/wallet.md) | Wallet |

## Notes

- ⚠️ **Prod transactions** — always ask user to type `CONFIRM` before executing.
- Append `--profile <name>` to any command to use a non-active profile.
- All authenticated endpoints accept optional `--recvWindow <ms>` (max 60 000).
- Timestamps (`startTime`, `endTime`) are Unix ms.
- For endpoints not listed in the skill, use `binance-cli request (GET|POST|PUT...) <url> [--signed]`. Any Parameters can be added to the request (e.g: `--param1 value --param2 value`).
