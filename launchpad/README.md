# Saturn launchpad beta

From `launchpad/` with Node.js 22.12 or newer:

```sh
npm ci
npm run dev
```

Open `/launchpad/` on the local URL printed by Vite. `npm run build` outputs the static site to `dist/`.

The discovery terminal uses React, Tailwind CSS, and a locally bundled Lucide sprite. Edit the theme in the opening `:root` block of `src/terminal.css`. The terminal starts with nine fictional markets. Quotes, wallet balances, launches, activity, and DEX graduations are simulated in the browser. The demo uses a fixed conversion of 0.24 USD per XLM and saves the session in local storage.

### Stellar testnet tools

The existing Freighter workflow is available at `/launchpad/testnet/`. The front page's embedded quote preview opens this workflow automatically.

The testnet tools are pinned to Stellar testnet and the factory from `saturn-testnet-run`. Load any token registered with that factory by contract ID. The seeded testnet project loads on startup. A funded testnet Freighter account is required to simulate a write. The review shows the simulation's network fee and operation details. Freighter prompts separately before the app submits any signed transaction. Trade limits use the displayed quote exactly, so an adverse price movement requires a fresh simulation.

The staking actions queue a deposit, advance the vault, settle one completed reward day, and claim up to sixteen settled epochs per transaction. Epoch settlement follows the on-chain clock. The app currently reports when a separate archival restore is required. Event indexing, keeper funding and monitoring, and wallet-assisted restores are separate beta work.

### Genesis homepage mint

Configure the public frontend build with `VITE_GENESIS_CONTRACT` (the deployed GenesisSale contract ID), `VITE_GENESIS_NETWORK` (`testnet` or `public`), and `VITE_GENESIS_RPC_URL` (an HTTPS, browser-accessible Stellar RPC endpoint for that network). These values are public. With no sale ID the homepage shows its prelaunch state. The wallet review reads the contract price, verifies native XLM, and pins the next NFT ID before signing. The sale contract must already be initialized with the intended treasury, fixed price, and 100 metadata entries before enabling the public mint.
