# Saturn launchpad beta

From `launchpad/` with Node.js 22.12 or newer:

```sh
npm ci
npm run dev
```

Open the local URL printed by Vite. `npm run build` outputs the static app to `dist/`.

The app is pinned to Stellar testnet and the factory from `saturn-testnet-run`. Load any token registered with that factory by contract ID. The seeded testnet project loads on startup. A funded testnet Freighter account is required to simulate a write. The review shows the simulation’s network fee and operation details. Freighter prompts separately before the app submits any signed transaction. Trade limits use the displayed quote exactly, so an adverse price movement requires a fresh simulation.

The staking actions queue a deposit, advance the vault, settle one completed reward day, and claim up to sixteen settled epochs per transaction. Epoch settlement follows the on-chain clock. The app currently reports when a separate archival restore is required. Event indexing, keeper funding and monitoring, and wallet-assisted restores are separate beta work.
