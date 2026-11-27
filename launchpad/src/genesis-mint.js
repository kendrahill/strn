// Build-time settings identify the sale. Price and availability always come from its contract.
const contract = import.meta.env.VITE_GENESIS_CONTRACT;
const rpcUrl = import.meta.env.VITE_GENESIS_RPC_URL;
const environment = import.meta.env.VITE_GENESIS_NETWORK;
const el = (id) => document.getElementById(id);
const hex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
const button = el('purchase');
const status = el('mint-status');

async function start() {
  const {
    Account,
    Address,
    Asset,
    BASE_FEE,
    Contract,
    Networks,
    TransactionBuilder,
    nativeToScVal,
    rpc,
    scValToNative,
  } = await import('@stellar/stellar-sdk');
  const wallet = await import('@stellar/freighter-api');
  if (!['public', 'testnet'].includes(environment) || !rpcUrl)
    throw new Error('Sale configuration is incomplete.');
  const passphrase = environment === 'public' ? Networks.PUBLIC : Networks.TESTNET;
  const networkName = environment === 'public' ? 'Stellar mainnet' : 'Stellar testnet';
  el('network').textContent = networkName;
  const server = new rpc.Server(rpcUrl);
  const sale = new Contract(contract);
  const source = new Account('GA67VLRLS7UVRVAXKI77IFNH6AIHN6TWFGO3USTVR3CMA26YPEL6B6BV', '0');
  const pendingKey = 'saturn.genesis.' + environment + '.' + contract;
  let hash = sessionStorage.getItem(pendingKey);
  let prepared;
  function xlm(stroops) {
    const n = BigInt(stroops);
    const decimals = (n % 10000000n).toString().padStart(7, '0').replace(/0+$/, '');
    return (n / 10000000n).toLocaleString('en-US') + (decimals ? '.' + decimals : '') + ' XLM';
  }
  async function simulate(account, method, ...args) {
    const tx = new TransactionBuilder(account, { networkPassphrase: passphrase, fee: BASE_FEE })
      .addOperation(sale.call(method, ...args))
      .setTimeout(180)
      .build();
    const sim = await server.simulateTransaction(tx);
    if (rpc.Api.isSimulationError(sim) || rpc.Api.isSimulationRestore(sim) || !sim.result)
      throw new Error(
        'The sale could not confirm this request. Refresh availability and try again.'
      );
    return { tx, sim, value: scValToNative(sim.result.retval) };
  }
  async function refresh() {
    const [net, config, supply, max] = await Promise.all([
      server.getNetwork(),
      simulate(source, 'config'),
      simulate(source, 'total_supply'),
      simulate(source, 'max_supply'),
    ]);
    if (
      net.passphrase !== passphrase ||
      config.value.xlm !== Asset.native().contractId(passphrase) ||
      max.value !== 100
    )
      throw new Error('The configured sale could not be verified.');
    const count = Number(supply.value);
    el('count').textContent = count + ' / 100';
    el('price').textContent = xlm(config.value.price);
    el('progress').value = count;
    el('progress').hidden = false;
    button.disabled = count >= 100;
    button.textContent = count >= 100 ? 'Collection minted out' : 'Review mint in XLM';
    return count;
  }
  async function check() {
    const receipt = await server.getTransaction(hash);
    if (receipt.status === 'NOT_FOUND') {
      button.textContent = 'Check transaction';
      button.disabled = false;
      status.textContent = 'Awaiting a ledger result. Transaction ' + hash;
      return;
    }
    const completed = hash;
    hash = null;
    sessionStorage.removeItem(pendingKey);
    await refresh();
    status.textContent =
      (receipt.status === 'SUCCESS' ? 'Genesis minted to your wallet. ' : 'Mint failed. ') +
      'Transaction ' +
      completed;
  }
  async function account() {
    const access = await wallet.requestAccess();
    if (access.error || !access.address)
      throw new Error(access.error?.message || 'Connect a funded Freighter wallet.');
    const network = await wallet.getNetworkDetails();
    if (network.networkPassphrase !== passphrase)
      throw new Error('Switch Freighter to ' + networkName + ' and try again.');
    return access.address;
  }
  async function act() {
    button.disabled = true;
    try {
      if (hash) return await check();
      if (!prepared) {
        if ((await refresh()) >= 100) return;
        button.disabled = true;
        const owner = await account();
        const { value: quote } = await simulate(source, 'quote');
        const { tx, sim } = await simulate(
          await server.getAccount(owner),
          'buy',
          Address.fromString(owner).toScVal(),
          nativeToScVal(quote.token_id, { type: 'u32' }),
          nativeToScVal(quote.price, { type: 'i128' }),
          nativeToScVal(BigInt(Math.floor(Date.now() / 1000) + 180), { type: 'u64' })
        );
        const transaction = rpc.assembleTransaction(tx, sim).build();
        prepared = { transaction, owner };
        status.textContent =
          'Genesis #' +
          quote.token_id +
          ' costs ' +
          xlm(quote.price) +
          '. Maximum network fee ' +
          xlm(transaction.fee) +
          '. Review your purchase in Freighter.';
        button.textContent = 'Confirm mint in Freighter';
      } else {
        const { transaction, owner } = prepared;
        prepared = null;
        if (owner !== (await account())) throw new Error('Wallet changed. Review the mint again.');
        const signed = await wallet.signTransaction(transaction.toXDR(), {
          address: owner,
          networkPassphrase: passphrase,
        });
        if (signed.error || !signed.signedTxXdr)
          throw new Error(signed.error?.message || 'Signature cancelled.');
        const envelope = TransactionBuilder.fromXDR(signed.signedTxXdr, passphrase);
        if (
          !envelope.signatures.length ||
          hex(envelope.hash()) !== hex(transaction.hash())
        )
          throw new Error('Wallet returned an unexpected transaction.');
        hash = hex(envelope.hash());
        sessionStorage.setItem(pendingKey, hash);
        const result = await server.sendTransaction(envelope);
        if (['ERROR', 'TRY_AGAIN_LATER'].includes(result.status)) {
          hash = null;
          sessionStorage.removeItem(pendingKey);
          throw new Error('Mint was not accepted. Review a fresh quote.');
        }
        return await check();
      }
      button.disabled = false;
    } catch (error) {
      prepared = null;
      button.textContent = hash ? 'Check transaction' : 'Refresh and review mint';
      status.textContent = hash ? 'Check the ledger result for transaction ' + hash : error.message;
      button.disabled = false;
    }
  }
  button.addEventListener('click', act);
  if (hash) return act();
  try {
    const count = await refresh();
    status.textContent =
      count === 100
        ? 'All 100 Genesis NFTs have been minted.'
        : 'Connect Freighter to review one Genesis NFT on ' + networkName + '.';
  } catch (error) {
    status.textContent = error.message;
    button.textContent = 'Refresh availability';
    button.disabled = false;
  }
}
if (contract)
  start().catch(() => {
    status.textContent = 'Mint information is temporarily unavailable.';
  });
