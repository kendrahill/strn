# Saturn Genesis NFT sale

One Soroban contract manages collection ownership, sales, and transfers. The existing native XLM Stellar Asset Contract handles payments. The deployment configuration pins the payment contract, treasury, and fixed price in stroops. Verify the canonical native XLM contract for the selected network before deployment.

`metadata.json` is the ordered constructor array. It references the verified Arweave manifest paths for tokens 1–100. The constructor accepts exactly 100 unique Arweave references. There is no separate mint, metadata edit, or withdrawal entry point.

The sale app reads `quote()` and passes the quoted token ID and maximum acceptable price to `buy(buyer, expected_token_id, max_price, deadline)`. The deadline is a Unix timestamp in seconds. If another buyer takes the quoted token first, refresh the quote before requesting a new signature. A successful purchase sends the payment straight to the treasury and mints to the buyer in the same transaction.

Run `cargo test --locked --lib` and `stellar contract build --locked` from this directory. The bundled WASM is a local build. A deployment still needs the selected network, treasury address, and sale price.
