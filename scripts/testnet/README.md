# Saturn launchpad testnet smoke test

Run from the extracted source package root with Node.js 22+, Stellar CLI 28.1.0, and Rust with the `wasm32v1-none` target. The script targets the public Stellar testnet RPC and checks its network passphrase before any network write.

```sh
rustup target add wasm32v1-none
node scripts/testnet/lifecycle.mjs verify
node scripts/testnet/lifecycle.mjs check-network
```

`verify` builds and hashes the five deployable WASMs and the local market fixture, then runs the governance, token, vault, and factory test suites, including the WASM lifecycle tests. `build` only builds the five deployable WASMs. Outputs go in `.saturn-build`. Retain the same build files for the testnet run to keep the locally tested hashes. The native XLM SAC derived for testnet is `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`.

## Submit the smoke test

Use a dedicated local Stellar CLI identity with at least **31,000 spendable testnet XLM**. This is a smoke-test budget for the 30,000 XLM gross curve target, the 100 XLM creation fee, trading and LP principal, and room for transaction fees. It is separate from the actual network fee. The three fee recipients must be distinct existing testnet accounts with XLM balances, each different from the signer. Use a testnet account as the STRN reward recipient until the core reward vault is available. The signer also acts as governance owner, creator, trader, LP, and staker. Beneficiaries do not need to sign.

```sh
node scripts/testnet/lifecycle.mjs start SIGNER TREASURY FOUNDATION REWARDS ./saturn-testnet-run --submit-testnet
```

The run directory must be new. The workflow uploads the WASMs, deploys governance and the factory, reads back governance and the factory template, and creates a project with a fresh salt. It confirms the creation payment, buys and redeems on the curve, and queues stake before the closing buy. It leaves 0.199 project tokens of dust to exercise the burn path, then checks the graduation fee, AMM seed reserves, and locked shares. A swap and round-trip LP deposit and withdrawal check reserve and share accounting.

The testnet account must fund the full launch and trading sequence at the default governance settings. If the canonical native SAC is absent after a reset, deploy it first with an authorized testnet identity using `stellar contract asset deploy --asset native --network testnet --source-account SIGNER`.

## Finish staking

The first effective staking day starts at the UTC boundary after queueing. Wait until that entire day has closed on the testnet ledger. `start` prints the earliest finish time. A run can therefore span two UTC boundaries.

```sh
node scripts/testnet/lifecycle.mjs finish SIGNER ./saturn-testnet-run --submit-testnet
```

The second phase rolls through the effective day, settles completed epochs, claims the sole staker’s initial and recycled rewards, and checks the token supply identity. Run it once for a successful smoke test.

## Read the measurements

`transactions.jsonl` records the factory launch and closing trade separately. The `assembled` entries include the transaction hash, simulation ledger, minimum resource fee in stroops, and decoded Soroban transaction data with instruction budget, footprint, and read/write bytes. The confirmed entries record the ledger, decoded submitted envelope, fee charged as reported in the transaction result, and raw transaction fee events when the RPC provides them. Use the fee events to account for refunds where available. Compare the submitted resource budget in the envelope with simulation and the current network limits. The 100 XLM creation payment and 0.5% graduation payment are protocol charges.

On interruption, inspect the recorded contract addresses and transaction hashes before starting another run. The launch salt and predicted children are logged before the factory call. The script stops on a failed preflight, rejected simulation, rejected broadcast, or mismatched invariant. A new run deploys a fresh factory and starts with a new salt.

Local integration tests print host CPU, memory, storage, and event resource estimates for the all-WASM factory launch and atomic graduation. These estimates omit network overhead and cannot establish the live inclusion fee or whether the paths fit current testnet limits.

[Stellar RPC simulation](https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/simulateTransaction) · [Transaction results](https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getTransaction)
