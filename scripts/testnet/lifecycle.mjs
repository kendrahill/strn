import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { setTimeout as pause } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const wasmDir = join(root, '.saturn-build');
const passphrase = 'Test SDF Network ; September 2015';
const rpcUrl = 'https://soroban-testnet.stellar.org';
const network = ['--rpc-url', rpcUrl, '--network-passphrase', passphrase];
const UNIT = 10_000_000n;
const SALE = 400_000_000n * UNIT;
const SEED = 300_000_000n * UNIT;
const FEE = 100n * UNIT;
const TARGET = 30_000n * UNIT;
const STAKE = 1_000n * UNIT;
const DAY = 86_400n;
const accountPattern = /^G[A-Z2-7]{55}$/;
const contractPattern = /^C[A-Z2-7]{55}$/;
const hashPattern = /^[0-9a-f]{64}$/;
const names = ['launch-fees', 'project-token', 'staking-vault', 'project-market', 'launch-factory'];
const cliEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) =>
  !/^STELLAR_/i.test(key) || key.toUpperCase() === 'STELLAR_CONFIG_DIR'
));

function run(command, args, { input, inherit = false, env = cliEnv } = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    env,
    input,
    encoding: 'utf8',
    maxBuffer: 8 * 1024 * 1024,
    stdio: inherit ? 'inherit' : ['pipe', 'pipe', 'pipe'],
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.slice(0, 2).join(' ')} failed: ${result.stderr?.trim() ?? result.status}`);
  }
  return result.stdout?.trim();
}

function stellar(args, input) {
  return run('stellar', ['--no-cache', ...args], { input });
}

function artifact(name) {
  const file = join(wasmDir, `saturn_${name.replaceAll('-', '_')}.wasm`);
  const bytes = readFileSync(file);
  assert(WebAssembly.validate(bytes), `Invalid WASM: ${file}`);
  return { file, hash: createHash('sha256').update(bytes).digest('hex') };
}

function build(verify) {
  mkdirSync(wasmDir, { recursive: true });
  const projects = names.map(name => [name, `contracts/${name}`]);
  if (verify) projects.push(['market-test-fixture', 'contracts/launch-factory/test-fixture']);
  for (const [name, path] of projects) {
    run('stellar', ['contract', 'build', '--locked', '--optimize=false',
      '--manifest-path', `${path}/Cargo.toml`, '--out-dir', wasmDir], { inherit: true });
    console.log(name, artifact(name).hash);
  }
  console.log('Testnet native XLM SAC', nativeXlm());
  if (verify) {
    const env = { ...cliEnv, SATURN_WASM_DIR: wasmDir, SATURN_PROJECT_TOKEN_WASM: artifact('project-token').file };
    for (const name of ['launch-fees', 'project-token', 'staking-vault', 'launch-factory']) {
      run('cargo', ['test', '--lib', '--locked', '--manifest-path', `contracts/${name}/Cargo.toml`, '--',
        '--include-ignored', '--nocapture'],
      { inherit: true, env });
    }
  }
}

async function rpc(method, params = {}) {
  const response = await fetch(rpcUrl, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    signal: AbortSignal.timeout(30_000),
  });
  if (!response.ok) throw new Error(`${method}: HTTP ${response.status}`);
  const body = await response.json();
  if (body.error) throw new Error(`${method}: ${JSON.stringify(body.error)}`);
  return body.result;
}

async function verifyNetwork() {
  assert.equal((await rpc('getNetwork')).passphrase, passphrase, 'Unexpected RPC network');
}

function nativeXlm() {
  const address = stellar(['contract', 'id', 'asset', '--asset', 'native', ...network]);
  assert.match(address, contractPattern);
  return address;
}

function signerAddress(signer) {
  assert.match(signer ?? '', /^(?![SC][A-Z2-7]{55}$)[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$/,
    'Use a local Stellar CLI identity alias');
  const address = stellar(['keys', 'address', signer]);
  assert.match(address, accountPattern);
  return address;
}

function invokeArgs(contract, signer, method, args, flags) {
  const encoded = Object.entries(args).flatMap(([key, value]) => [
    `--${key}`, typeof value === 'object' ? JSON.stringify(value) : String(value),
  ]);
  return ['contract', 'invoke', ...network, '--contract-id', contract, '--source-account', signer,
    ...flags, '--', method, ...encoded];
}

function invoke(contract, signer, method, args = {}, send = false) {
  const output = stellar(invokeArgs(contract, signer, method, args, [`--send=${send ? 'yes' : 'no'}`]));
  return output ? JSON.parse(output) : null;
}

function deploy(signer, artifactHash, args) {
  const address = stellar(['contract', 'deploy', ...network, '--source-account', signer,
    '--wasm-hash', artifactHash, '--', ...args]);
  assert.match(address, contractPattern);
  return address;
}

function record(directory, entry) {
  appendFileSync(join(directory, 'transactions.jsonl'), JSON.stringify(entry) + '\n');
  console.log(entry.step, entry.hash ?? entry.address ?? entry.status ?? '');
}

async function measuredInvoke(ctx, step, contract, method, args) {
  const unsigned = stellar(invokeArgs(contract, ctx.owner, method, args, ['--build-only']));
  const simulation = await rpc('simulateTransaction', { transaction: unsigned });
  assert(!simulation.error && !simulation.restorePreamble && simulation.transactionData,
    `${step} simulation failed: ${JSON.stringify(simulation)}`);
  const resources = JSON.parse(stellar(['xdr', 'decode', '--type', 'SorobanTransactionData',
    '--input', 'single-base64', simulation.transactionData]));

  // CLI assembles the footprint, fee and source-account authorization from a fresh simulation.
  const assembled = stellar(['tx', 'simulate', ...network, '--source-account', ctx.owner], unsigned);
  const signed = stellar(['tx', 'sign', ...network, '--sign-with-key', ctx.signer], assembled);
  const hash = stellar(['tx', 'hash', ...network], signed);
  assert.match(hash, hashPattern);
  record(ctx.dir, { step, status: 'assembled', hash, simulationLedger: simulation.latestLedger,
    minResourceFee: simulation.minResourceFee, resources });
  const submitted = await rpc('sendTransaction', { transaction: signed });
  assert.equal(submitted.hash, hash);
  assert(['PENDING', 'DUPLICATE'].includes(submitted.status), JSON.stringify(submitted));
  for (let attempt = 0; attempt < 30; attempt++) {
    const tx = await rpc('getTransaction', { hash });
    if (tx.status === 'NOT_FOUND') {
      await pause(2000);
      continue;
    }
    const result = JSON.parse(stellar(['xdr', 'decode', '--type', 'TransactionResult',
      '--input', 'single-base64', tx.resultXdr]));
    const envelope = JSON.parse(stellar(['tx', 'decode'], tx.envelopeXdr));
    record(ctx.dir, { step, hash, status: tx.status, ledger: tx.ledger,
      feeChargedResult: result.fee_charged, transactionEventsXdr: tx.events?.transactionEventsXdr, envelope });
    assert.equal(tx.status, 'SUCCESS', `${step} failed on testnet: ${hash}`);
    return;
  }
  throw new Error(`${step} confirmation timed out. Check recorded transaction ${hash} before rerunning.`);
}

function bounds(quote) {
  return { max_input: quote.input, min_output: quote.output, deadline: Math.floor(Date.now() / 1000) + 600 };
}

function save(directory, state) {
  writeFileSync(join(directory, 'run.json'), JSON.stringify(state, null, 2) + '\n', { flag: 'wx' });
}

async function start(signer, treasury, foundation, rewards, directory) {
  const owner = signerAddress(signer);
  for (const address of [treasury, foundation, rewards]) {
    assert.match(address ?? '', accountPattern, 'Use existing testnet account addresses for fee recipients');
  }
  assert.equal(new Set([owner, treasury, foundation, rewards]).size, 4, 'Use distinct testnet accounts');
  const dir = resolve(directory);
  const ctx = { signer, owner, dir };
  const artifacts = Object.fromEntries(names.map(name => [name, artifact(name)]));
  await verifyNetwork();
  const xlm = nativeXlm();
  const read = (address, method, args) => invoke(address, owner, method, args);
  const send = (address, method, args) => invoke(address, signer, method, args, true);
  const balance = (token, id) => BigInt(read(token, 'balance', { id }));
  assert(balance(xlm, owner) >= 31_000n * UNIT, 'Signer needs at least 31,000 spendable testnet XLM');
  for (const recipient of [treasury, foundation, rewards]) {
    assert(balance(xlm, recipient) > 0n, `Fund recipient ${recipient} on testnet first`);
  }
  mkdirSync(dir);
  const hashes = {};
  for (const [name, { file, hash }] of Object.entries(artifacts)) {
    hashes[name] = stellar(['contract', 'upload', ...network, '--source-account', signer, '--wasm', file]);
    assert.equal(hashes[name], hash, `Uploaded hash mismatch for ${name}`);
    record(dir, { step: 'upload ' + name, hash });
  }
  const governance = deploy(signer, hashes['launch-fees'], ['--owner', owner, '--treasury', treasury]);
  record(dir, { step: 'deploy governance', address: governance });
  const policy = read(governance, 'policy', { max_fee: FEE });
  assert.equal(policy.treasury, treasury);
  assert.equal(BigInt(policy.creation_fee), FEE);
  assert.equal(BigInt(read(governance, 'graduation_target')), TARGET);
  const config = { governance, xlm, strn_rewards: rewards, foundation, template: {
    token_wasm: hashes['project-token'], vault_wasm: hashes['staking-vault'],
    curve_wasm: hashes['project-market'], pool_wasm: hashes['project-market'],
  } };
  const factory = deploy(signer, hashes['launch-factory'], ['--config', JSON.stringify(config)]);
  record(dir, { step: 'deploy factory', address: factory });
  const observedConfig = read(factory, 'config');
  assert.deepEqual(observedConfig, config);

  const salt = randomBytes(32).toString('hex');
  const lookup = { creator: owner, salt };
  const predicted = read(factory, 'predict_launch', lookup);
  record(dir, { step: 'predict launch', salt, predicted });
  const treasuryBefore = balance(xlm, treasury);
  await measuredInvoke(ctx, 'factory launch', factory, 'create_launch', {
    creator: owner,
    metadata: { name: 'Saturn Testnet Smoke', symbol: 'SATS', description: '',
      image_uri: null, image_sha256: null, website_uri: null },
    request: { salt, max_creation_fee: String(FEE), expected_target: String(TARGET), lock_metadata: true },
  });

  const launch = read(factory, 'resolve_launch', lookup);
  assert.deepEqual(launch.addresses, predicted);
  const { token, vault, curve, pool } = launch.addresses;
  assert.deepEqual(read(factory, 'resolve_token', { token }), launch);
  assert.equal(balance(xlm, treasury) - treasuryBefore, FEE);
  assert.equal(balance(token, curve), SALE + SEED);
  assert.equal(read(vault, 'asset'), token);
  assert.equal(read(pool, 'reserves'), null);
  assert.equal(BigInt(launch.target), TARGET);
  assert.equal(read(token, 'metadata').locked, true);

  function curveTrade(buy, tokens) {
    const args = { buy, tokens };
    const quote = read(curve, 'quote_curve', args);
    assert.equal(quote.graduation, null);
    assert.deepEqual(send(curve, 'trade_curve', { trader: owner, ...args, ...bounds(quote) }), quote);
    return quote;
  }

  const initialBuy = curveTrade(true, 1_000_000n * UNIT);
  const sell = curveTrade(false, 100_000n * UNIT);
  assert.equal(balance(xlm, curve), BigInt(initialBuy.xlm) - BigInt(sell.xlm));
  assert.equal(balance(token, owner), BigInt(initialBuy.output) - BigInt(sell.input));
  send(vault, 'queue_stake', { owner, amount: STAKE });
  assert.equal(BigInt(read(vault, 'principal_liability')), STAKE);
  const position = read(vault, 'position', { owner });
  const effectiveDay = BigInt(position.day) + (BigInt(position.pending_deposit) === STAKE ? 1n : 0n);

  const sold = BigInt(read(curve, 'curve_state').sold);
  const closeArgs = { buy: true, tokens: SALE - sold - 1_990_000n };
  const close = read(curve, 'quote_curve', closeArgs);
  const graduation = close.graduation;
  assert(graduation, 'Closing quote must atomically graduate the curve');
  assert.equal(BigInt(graduation.dust), 1_990_000n);
  assert.equal(BigInt(graduation.fee), BigInt(graduation.gross_xlm) / 200n);
  assert.equal(BigInt(graduation.pool_xlm), BigInt(graduation.gross_xlm) - BigInt(graduation.fee));
  assert.equal(BigInt(graduation.pool_tokens), SEED);

  const beforeClose = balance(xlm, treasury);
  await measuredInvoke(ctx, 'atomic graduation', curve, 'trade_curve', {
    trader: owner, ...closeArgs, ...bounds(close),
  });
  assert.equal(read(curve, 'curve_state').graduated, true);
  assert.equal(balance(xlm, treasury) - beforeClose, BigInt(graduation.fee));
  assert.equal(balance(xlm, curve), 0n);
  assert.equal(balance(token, curve), 0n);
  assert.equal(BigInt(read(vault, 'principal_liability')), STAKE);
  const seeded = read(pool, 'reserves');
  assert.deepEqual(seeded, { tokens: graduation.pool_tokens, xlm: graduation.pool_xlm });
  const genesis = read(pool, 'share_supply');
  assert.equal(BigInt(genesis.total), BigInt(genesis.locked));
  assert(BigInt(genesis.locked) > 0n);

  const swapArgs = { buy: true, principal: 10n * UNIT };
  const swap = read(pool, 'quote_swap', swapArgs);
  assert.deepEqual(send(pool, 'swap', { trader: owner, ...swapArgs, ...bounds(swap) }), swap);
  const swapped = read(pool, 'reserves');
  assert(BigInt(swapped.xlm) * BigInt(swapped.tokens) >= BigInt(seeded.xlm) * BigInt(seeded.tokens));
  const max = { max_xlm: 10n * UNIT, max_tokens: 200_000n * UNIT };
  const liquidity = read(pool, 'quote_deposit', max);
  const deadline = Math.floor(Date.now() / 1000) + 600;
  assert.deepEqual(send(pool, 'deposit', { owner, ...max, min_shares: liquidity.shares, deadline }), liquidity);
  assert.equal(read(pool, 'lp_balance', { owner }), liquidity.shares);
  const exit = read(pool, 'quote_withdraw', { shares: liquidity.shares });
  assert.deepEqual(send(pool, 'withdraw', {
    owner, shares: exit.shares, min_xlm: exit.xlm, min_tokens: exit.tokens, deadline,
  }), exit);
  assert.equal(BigInt(read(pool, 'lp_balance', { owner })), 0n);
  assert.deepEqual(read(pool, 'share_supply'), genesis);
  const remaining = read(pool, 'reserves');
  assert.equal(balance(xlm, pool), BigInt(remaining.xlm));
  assert.equal(balance(token, pool), BigInt(remaining.tokens));

  save(dir, { passphrase, owner, factory, governance, launch, effectiveDay: String(effectiveDay) });
  console.log('Market smoke passed. Finish staking after', new Date(Number((effectiveDay + 1n) * DAY) * 1000).toISOString());
}

async function finish(signer, directory) {
  const owner = signerAddress(signer);
  const dir = resolve(directory);
  const state = JSON.parse(readFileSync(join(dir, 'run.json'), 'utf8'));
  assert.equal(state.passphrase, passphrase);
  assert.equal(state.owner, owner);
  await verifyNetwork();
  const { vault, token } = state.launch.addresses;
  assert.match(vault, contractPattern);
  assert.match(token, contractPattern);
  const read = (address, method, args) => invoke(address, owner, method, args);
  const send = (method, args) => invoke(vault, signer, method, args, true);
  const day = BigInt(state.effectiveDay);
  const ledgerTime = BigInt((await rpc('getLatestLedger')).closeTime);
  assert(ledgerTime / DAY > day, 'Wait until the first eligible UTC staking day finishes');
  const currentPool = read(vault, 'pool');
  if (BigInt(currentPool.day) < day) send('roll', { max_days: Number(day - BigInt(currentPool.day)) });
  const power = BigInt(read(vault, 'day_power', { day }));
  assert.equal(power, STAKE * 180n);
  let nextDay = BigInt(read(vault, 'reward_state').next_day);
  assert(day - nextDay <= 3n, 'Expected only the initial smoke-test reward epochs');
  while (nextDay <= day) {
    const epoch = send('settle_rewards');
    assert.equal(BigInt(epoch.day), nextDay);
    nextDay += 1n;
  }
  const epoch = read(vault, 'reward_epoch', { day });
  assert.equal(BigInt(epoch.power), power);
  assert(BigInt(epoch.minted) > 0n && BigInt(epoch.recycled) > 0n);
  const expectedClaim = invoke(vault, owner, 'claim', { owner, max_days: 16 });
  assert.equal(BigInt(expectedClaim.paid), BigInt(epoch.minted), 'Expected one unclaimed eligible epoch');
  const beforeClaim = BigInt(read(token, 'balance', { id: owner }));
  const claim = send('claim', { owner, max_days: 16 });
  assert.equal(BigInt(claim.paid), BigInt(epoch.minted));
  assert.equal(BigInt(read(token, 'balance', { id: owner })) - beforeClaim, BigInt(claim.paid));
  const supply = read(token, 'supply');
  assert.equal(BigInt(supply.outstanding) + BigInt(supply.unused_initial) + BigInt(supply.unused_recycled),
    1_000_000_000n * UNIT);
  record(dir, { step: 'staking and claim', status: 'SUCCESS', epoch, claim });
}

const [mode, ...args] = process.argv.slice(2);
try {
  if ((mode === 'build' || mode === 'verify') && args.length === 0) {
    build(mode === 'verify');
  } else if (mode === 'check-network' && args.length === 0) {
    await verifyNetwork();
    console.log('Verified testnet RPC and native XLM SAC', nativeXlm());
  } else if (mode === 'start' && args.length === 6 && args.pop() === '--submit-testnet') {
    await start(...args);
  } else if (mode === 'finish' && args.length === 3 && args.pop() === '--submit-testnet') {
    await finish(...args);
  } else {
    throw new Error('Usage: lifecycle.mjs build|verify|check-network, start SIGNER TREASURY FOUNDATION REWARDS NEW_DIR --submit-testnet, or finish SIGNER RUN_DIR --submit-testnet');
  }
} catch (error) {
  console.error(error.message, error.cause?.message ?? '');
  process.exitCode = 1;
}
