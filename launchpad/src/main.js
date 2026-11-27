import { getNetworkDetails, requestAccess, signTransaction } from '@stellar/freighter-api';
import { TransactionBuilder, scValToNative } from '@stellar/stellar-sdk';
import { amount, create, initialToken, network, project, quoteTrade, server, stake, trade, units, waitForTransaction } from './saturn.js';
import './style.css';

const element = (id) => document.getElementById(id);
const hex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
const form = element('action-form');
const previewMode = new URLSearchParams(window.location.search).get('preview') === '1';
if (previewMode) document.body.classList.add('preview-mode');
const tabs = [...document.querySelectorAll('[data-view]')];
const titles = { trade: 'Trade', stake: 'Stake and claim', create: 'Create project' };
const stakingLabels = { deposit: 'Queue stake', roll: 'Advance staking days', settle: 'Settle one reward day', claim: 'Claim rewards' };
let view = 'trade';
let currentProject;
let owner;
let prepared;
let busy = false;

function status(id, message, error = false) {
  const output = element(id);
  output.textContent = message;
  output.dataset.error = String(error);
}

function metrics(container, entries) {
  container.replaceChildren(...entries.map(([label, value]) => {
    const item = document.createElement('div');
    const term = document.createElement('dt');
    const detail = document.createElement('dd');
    term.textContent = label;
    detail.textContent = value;
    item.append(term, detail);
    return item;
  }));
}

function dismiss() {
  prepared = undefined;
  element('review').hidden = true;
  element('quote').hidden = true;
}

function updateInputs() {
  const direction = form.elements.namedItem('direction');
  if (direction) {
    const isPoolBuy = currentProject?.reserves && direction.value === 'buy';
    element('trade-amount-label').textContent = `Principal amount (${isPoolBuy ? 'XLM' : currentProject?.symbol ?? 'tokens'})`;
    element('simulate').textContent = owner ? 'Simulate transaction' : 'Get quote';
  }
  const stakeMode = form.elements.namedItem('stakeMode');
  if (stakeMode) {
    const deposit = stakeMode.value === 'deposit';
    element('stake-amount').hidden = !deposit;
    form.elements.namedItem('quantity').required = deposit;
  }
}

function renderForm() {
  tabs.forEach((button) => button.setAttribute('aria-pressed', String(button.dataset.view === view)));
  element('action-title').textContent = titles[view];
  const fields = {
    trade: `<div class="form-row">
      <div><label for="direction">Side</label><select id="direction" name="direction"><option value="buy">Buy</option><option value="sell">Sell</option></select></div>
      <div><label for="trade-amount" id="trade-amount-label">Principal amount</label><input id="trade-amount" name="quantity" inputmode="decimal" placeholder="0.00" required></div>
    </div>`,
    stake: `<div class="form-row">
      <div><label for="stake-mode">Action</label><select id="stake-mode" name="stakeMode">${Object.entries(stakingLabels).map(([value, label]) => `<option value="${value}">${label}</option>`).join('')}</select></div>
      <div id="stake-amount"><label for="stake-quantity">Token amount</label><input id="stake-quantity" name="quantity" inputmode="decimal"></div>
    </div>`,
    create: `<div class="form-row">
      <div><label for="name">Token name</label><input id="name" name="name" maxlength="64" required></div>
      <div><label for="symbol-input">Token symbol</label><input id="symbol-input" name="symbol" maxlength="16" required></div>
    </div>
    <div><label for="description">Description</label><textarea id="description" name="description" maxlength="512" rows="3"></textarea></div>`,
  };
  form.innerHTML = `${fields[view]}<button id="simulate" type="submit">Simulate transaction</button>`;
  updateInputs();
}

async function run(statusId, task) {
  if (busy) return;
  busy = true;
  const controls = [...document.querySelectorAll('button, input, select, textarea')];
  for (const control of controls) control.disabled = true;
  try {
    await task();
  } catch (error) {
    status(statusId, String(error.message ?? error).slice(0, 500), true);
  } finally {
    busy = false;
    for (const control of controls) control.disabled = false;
  }
}

async function loadProject(token) {
  dismiss();
  currentProject = undefined;
  element('project').hidden = true;
  status('market-status', 'Loading project from testnet…');
  currentProject = await project(token);
  const { name, symbol, reserves, state, target } = currentProject;
  element('project-title').textContent = name;
  element('symbol').textContent = symbol;
  element('phase').textContent = reserves ? 'Graduated AMM' : 'Bonding curve';
  metrics(element('metrics'), reserves ? [
    ['XLM reserve', `${units(reserves.xlm)} XLM`], ['Token reserve', `${units(reserves.tokens)} ${symbol}`],
  ] : [
    ['Sold on curve', `${units(state.sold)} ${symbol}`], ['Graduation target', `${units(target)} XLM`],
  ]);
  element('project').hidden = false;
  status('market-status', '');
  updateInputs();
}

async function assertWalletNetwork() {
  const details = await getNetworkDetails();
  if (details.error) throw new Error(details.error.message);
  if (details.networkPassphrase !== network) throw new Error('Switch Freighter to Stellar testnet and try again.');
}

element('connect').addEventListener('click', () => run('action-status', async () => {
  if (previewMode) return;
  dismiss();
  owner = undefined;
  updateInputs();
  element('connect').textContent = 'Connect Freighter';
  const access = await requestAccess();
  if (access.error || !access.address) throw new Error(access.error?.message ?? 'Open Freighter to connect a wallet.');
  await assertWalletNetwork();
  owner = access.address;
  updateInputs();
  element('connect').textContent = `${owner.slice(0, 6)}…${owner.slice(-4)}`;
  status('action-status', 'Testnet wallet connected.');
}));

element('lookup').addEventListener('submit', (event) => {
  event.preventDefault();
  const token = element('token').value;
  run('market-status', () => loadProject(token));
});

for (const button of tabs) button.addEventListener('click', () => {
  if (busy) return;
  view = button.dataset.view;
  dismiss();
  status('action-status', '');
  renderForm();
});

form.addEventListener('input', () => {
  dismiss();
  status('action-status', '');
  updateInputs();
});

function tradeDetails(quote, buy, isPrepared) {
  const token = currentProject.symbol;
  return [
    [isPrepared ? 'Maximum sent' : 'Estimated send', `${units(quote.input)} ${buy ? 'XLM' : token}`],
    [isPrepared ? 'Minimum received' : 'Estimated receive', `${units(quote.output)} ${buy ? token : 'XLM'}`],
    ['Trading fees included', `${units(quote.xlm_fee)} XLM + ${units(quote.token_fee)} ${token}`],
  ];
}

form.addEventListener('submit', (event) => {
  event.preventDefault();
  const values = new FormData(form);
  run('action-status', async () => {
    dismiss();
    if (view !== 'create' && !currentProject) throw new Error('Load a project first.');
    const buy = values.get('direction') === 'buy';
    if (view === 'trade' && !owner) {
      status('action-status', 'Getting a testnet quote…');
      const quote = await quoteTrade(currentProject, buy, amount(values.get('quantity')));
      metrics(element('quote-details'), tradeDetails(quote, buy, false));
      element('quote').hidden = false;
      status('action-status', previewMode ? 'Quote only. Open the full launchpad to trade.' : 'Connect Freighter to simulate a trade.');
      return;
    }
    if (!owner) throw new Error('Connect a Freighter testnet wallet to simulate this action.');
    await assertWalletNetwork();
    status('action-status', 'Simulating on testnet…');
    let details;
    if (view === 'trade') {
      prepared = await trade(currentProject, owner, buy, amount(values.get('quantity')));
      details = tradeDetails(prepared.quote, buy, true);
    } else if (view === 'stake') {
      const mode = values.get('stakeMode');
      const quantity = mode === 'deposit' ? amount(values.get('quantity')) : undefined;
      prepared = await stake(currentProject, owner, mode, quantity);
      details = [['Action', stakingLabels[mode]]];
      if (mode === 'deposit') details.push(['Queued principal', `${units(quantity)} ${currentProject.symbol}`]);
      if (mode === 'claim') details.push(['Simulated payout', `${units(prepared.result.paid)} ${currentProject.symbol}`]);
    } else {
      prepared = await create(owner, values.get('name').trim(), values.get('symbol').trim(), values.get('description').trim());
      details = [['Creation fee', `${units(prepared.result.creation_fee)} XLM`], ['Graduation target', `${units(prepared.result.target)} XLM`]];
    }
    details.push(['Maximum network fee', `${units(prepared.transaction.fee)} XLM`]);
    metrics(element('details'), details);
    element('review').hidden = false;
    status('action-status', 'Simulation succeeded. Review the amounts before signing.');
  });
});

element('dismiss').addEventListener('click', () => {
  dismiss();
  status('action-status', 'Review dismissed.');
});

element('submit').addEventListener('click', () => run('action-status', async () => {
  if (!prepared) return;
  const { transaction, owner: signer } = prepared;
  await assertWalletNetwork();
  if (Number(transaction.timeBounds.maxTime) <= Math.floor(Date.now() / 1000)) {
    dismiss();
    throw new Error('This simulation expired. Run it again before signing.');
  }
  status('action-status', 'Review the testnet transaction in Freighter.');
  const signed = await signTransaction(transaction.toXDR(), { networkPassphrase: network, address: signer });
  if (signed.error || !signed.signedTxXdr) throw new Error(signed.error?.message ?? 'Freighter did not sign the transaction.');
  const envelope = TransactionBuilder.fromXDR(signed.signedTxXdr, network);
  const hash = hex(transaction.hash());
  if (hex(envelope.hash()) !== hash || !envelope.signatures.length) {
    throw new Error('Freighter returned an unexpected transaction. Simulate again.');
  }
  dismiss();
  status('action-status', `Submitting to testnet…\n${hash}`);
  let submission;
  try {
    submission = await server.sendTransaction(envelope);
  } catch {
    throw new Error(`Submission status is unknown. Check this hash before trying again.\n${hash}`);
  }
  if (submission.status === 'ERROR' || submission.status === 'TRY_AGAIN_LATER') {
    throw new Error(`Testnet returned ${submission.status}. Simulate again before retrying.\n${hash}`);
  }
  let result;
  try {
    result = await waitForTransaction(hash);
  } catch {
    throw new Error(`Could not confirm this transaction. Check its hash before trying again.\n${hash}`);
  }
  if (!result) throw new Error(`Testnet has not confirmed this transaction yet.\n${hash}`);
  if (result.status !== 'SUCCESS') throw new Error(`Testnet returned ${result.status}.\n${hash}`);
  status('action-status', `Confirmed on testnet.\n${hash}`);
  const token = view === 'create' && result.returnValue
    ? scValToNative(result.returnValue).addresses.token
    : currentProject?.addresses.token;
  if (token) {
    element('token').value = token;
    try {
      await loadProject(token);
    } catch (error) {
      status('market-status', `Transaction confirmed. Project refresh failed. ${error.message}`, true);
    }
  }
}));

renderForm();
element('token').value = initialToken;
run('market-status', () => loadProject(initialToken));
