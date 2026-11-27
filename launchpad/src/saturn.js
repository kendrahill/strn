import { Account, Address, BASE_FEE, Contract, Networks, StrKey, TransactionBuilder, nativeToScVal, rpc, scValToNative, xdr } from '@stellar/stellar-sdk';

export const network = Networks.TESTNET;
export const server = new rpc.Server('https://soroban-testnet.stellar.org');
export const factory = 'CDKMSB5DKFCHUTDENWTO3BUND5F3NUWC4QRVZWM2RQSCYLCJH5BZ3KDM';
export const initialToken = 'CBBN5R44YCB5CSPWMR7P4ERNSDL6XC6WIDWD3DPLTTXRIW6PZUC46M73';
const readSource = 'GA67VLRLS7UVRVAXKI77IFNH6AIHN6TWFGO3USTVR3CMA26YPEL6B6BV';
const unit = 10_000_000n;
const i128Max = (1n << 127n) - 1n;

export function amount(text) {
  const match = /^(\d{1,39})(?:\.(\d{1,7}))?$/.exec(text.trim());
  if (!match) throw new Error('Enter an amount with up to seven decimal places.');
  const value = BigInt(match[1]) * unit + BigInt((match[2] ?? '').padEnd(7, '0'));
  if (value <= 0n || value > i128Max) throw new Error('Enter a positive amount within the supported range.');
  return value;
}

export function units(value) {
  const integer = BigInt(value);
  const absolute = integer < 0n ? -integer : integer;
  const fractional = (absolute % unit).toString().padStart(7, '0').replace(/0+$/, '');
  return `${integer < 0n ? '−' : ''}${(absolute / unit).toLocaleString('en-US')}${fractional ? `.${fractional}` : ''}`;
}

const address = (value) => Address.fromString(value).toScVal();
const int = (value) => nativeToScVal(value, { type: 'i128' });
const day = (value) => nativeToScVal(value, { type: 'u64' });
const bool = (value) => xdr.ScVal.scvBool(value);
const string = (value) => xdr.ScVal.scvString(value);
const struct = (fields) => xdr.ScVal.scvMap(Object.keys(fields).sort().map((key) =>
  new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol(key), val: fields[key] }),
));

async function simulate(account, contract, method, args) {
  const transaction = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: network })
    .addOperation(new Contract(contract).call(method, ...args))
    .setTimeout(300)
    .build();
  const response = await server.simulateTransaction(transaction);
  if (rpc.Api.isSimulationError(response)) throw new Error(`${method} simulation failed. ${response.error}`);
  if (!response.result) throw new Error(`${method} returned no simulation result.`);
  return { transaction, response, result: scValToNative(response.result.retval) };
}

export async function read(contract, method, args = []) {
  const { result } = await simulate(new Account(readSource, '0'), contract, method, args);
  return result;
}

export async function project(token) {
  if (!StrKey.isValidContract(token)) throw new Error('Enter a valid Stellar contract ID.');
  const launch = await read(factory, 'resolve_token', [address(token)]);
  if (!launch) throw new Error('This token was not found in the testnet factory.');
  const { curve, pool } = launch.addresses;
  const [name, symbol, reserves, state] = await Promise.all([
    read(token, 'name'), read(token, 'symbol'), read(pool, 'reserves'), read(curve, 'curve_state'),
  ]);
  return { ...launch, name, symbol, reserves, state };
}

export async function preview(owner, contract, method, args) {
  const account = await server.getAccount(owner);
  const { transaction, response, result } = await simulate(account, contract, method, args);
  if (rpc.Api.isSimulationRestore(response)) {
    throw new Error('Archived state requires a separate restoration before this action.');
  }
  return { transaction: rpc.assembleTransaction(transaction, response).build(), result, owner };
}

async function marketQuote(project, buy, quantity) {
  const pool = Boolean(project.reserves);
  const contract = pool ? project.addresses.pool : project.addresses.curve;
  const args = [bool(buy), int(quantity)];
  const quote = await read(contract, pool ? 'quote_swap' : 'quote_curve', args);
  return { pool, contract, args, quote };
}

export async function quoteTrade(project, buy, quantity) {
  return (await marketQuote(project, buy, quantity)).quote;
}

export async function trade(project, owner, buy, quantity) {
  const { pool, contract, args, quote } = await marketQuote(project, buy, quantity);
  const prepared = await preview(owner, contract, pool ? 'swap' : 'trade_curve', [
    address(owner), ...args, int(quote.input), int(quote.output), day(BigInt(Math.floor(Date.now() / 1000) + 300)),
  ]);
  return { ...prepared, quote, buy };
}

export async function create(owner, name, symbol, description) {
  const encoder = new TextEncoder();
  if (!name || !symbol || encoder.encode(name).length > 64 || encoder.encode(symbol).length > 16
    || encoder.encode(description).length > 512) {
    throw new Error('Enter a name and symbol within the 64- and 16-byte limits. Description is limited to 512 bytes.');
  }
  const { governance } = await read(factory, 'config');
  const [policy, target] = await Promise.all([
    read(governance, 'policy', [int(i128Max)]), read(governance, 'graduation_target'),
  ]);
  const metadata = struct({
    name: string(name), symbol: string(symbol), description: string(description),
    image_uri: xdr.ScVal.scvVoid(), image_sha256: xdr.ScVal.scvVoid(), website_uri: xdr.ScVal.scvVoid(),
  });
  const request = struct({
    salt: xdr.ScVal.scvBytes(crypto.getRandomValues(new Uint8Array(32))), max_creation_fee: int(policy.creation_fee),
    expected_target: int(target), lock_metadata: bool(false),
  });
  return preview(owner, factory, 'create_launch', [address(owner), metadata, request]);
}

export async function stake(project, owner, mode, quantity) {
  const vault = project.addresses.vault;
  if (mode === 'deposit') return preview(owner, vault, 'queue_stake', [address(owner), int(quantity)]);
  if (mode === 'roll') return preview(owner, vault, 'roll', [xdr.ScVal.scvU32(16)]);
  if (mode === 'settle') return preview(owner, vault, 'settle_rewards', []);
  const prepared = await preview(owner, vault, 'claim', [address(owner), xdr.ScVal.scvU32(16)]);
  if (BigInt(prepared.result.paid) === 0n) throw new Error('No settled reward is claimable in the next 16 epochs.');
  return prepared;
}

export async function waitForTransaction(hash) {
  for (let attempt = 0; attempt < 25; attempt += 1) {
    const transaction = await server.getTransaction(hash);
    if (transaction.status !== 'NOT_FOUND') return transaction;
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
  return null;
}
