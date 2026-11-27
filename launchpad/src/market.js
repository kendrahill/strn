import addresses from './demo-addresses.json' with { type: 'json' };

export const TARGET = 25000;
export const XLM_USD = 0.24;
export const WALLET = addresses[11].creator;
export const PRESETS = ['Standard Linear', 'Exponential', 'Steep'];
export const DEMO_KEY = 'saturn.discovery.v1';
const CURVE_SUPPLY = 1_000_000;
const TOTAL_SUPPLY = 3_000_000;
const VIRTUAL_SUPPLY = CURVE_SUPPLY / 4;
const fixtures = [
  ['Aurora Protocol', 'AURA', 'aurora', 94.6, 38.42, 1284, 14820, 380, 1],
  ['Nova Network', 'NOVA', 'nova', 68.4, 26.18, 862, 9120, 140, 0],
  ['Orbital', 'ORBT', 'orbital', 82.1, 19.76, 956, 12300, 620, 1],
  ['Prism Finance', 'PRSM', 'prism', 46.8, 14.93, 423, 6400, 95, 2],
  ['Helios', 'HELI', 'helios', 58.2, 11.85, 672, 8200, 250, 0],
  ['Vortex Labs', 'VRTX', 'vortex', 35.6, 8.64, 317, 4820, 52, 1],
  ['Lunar Relay', 'LUNA', 'lunar', 23.2, -3.18, 214, 3100, 28, 0],
  ['Cobalt', 'COBL', 'cobalt', 100, 6.74, 2118, 22750, 2820, 2],
  ['Vector Mesh', 'VCTR', 'vector', 12.6, -1.42, 126, 2100, 16, 0],
];

export const format = (n, digits = 0) =>
  Number(n).toLocaleString('en-US', { maximumFractionDigits: digits });
export const short = (s) => s.slice(0, 4) + '…' + s.slice(-4);
export const age = (time) => {
  const m = Math.max(0, Math.floor((Date.now() - time) / 60000));
  return m < 1
    ? 'just now'
    : m < 60
    ? m + 'm ago'
    : m < 1440
    ? Math.floor(m / 60) + 'h ago'
    : Math.floor(m / 1440) + 'd ago';
};
export const progress = (t) => Math.min(100, (t.raised / TARGET) * 100);
// Reserve is the integral of the preset's marginal price over circulating supply.
function shape(x, preset) {
  if (preset === 'Exponential') return Math.expm1(2 * x) / Math.expm1(2);
  if (preset === 'Steep') return (x + 3 * x ** 4) / 4;
  return (x + 2 * x * x) / 3;
}
export const reserveAt = (s, preset) => TARGET * shape(s / CURVE_SUPPLY, preset);
export function priceAt(s, preset) {
  const x = s / CURVE_SUPPLY;
  const slope =
    preset === 'Exponential'
      ? (2 * Math.exp(2 * x)) / Math.expm1(2)
      : preset === 'Steep'
      ? (1 + 12 * x ** 3) / 4
      : (1 + 4 * x) / 3;
  return (TARGET / CURVE_SUPPLY) * slope;
}
export function supplyAt(reserve, preset) {
  if (reserve <= 0) return 0;
  let low = 0,
    high = CURVE_SUPPLY;
  for (let i = 0; i < 48; i++) {
    const mid = (low + high) / 2;
    if (reserveAt(mid, preset) < reserve) low = mid;
    else high = mid;
  }
  return (low + high) / 2;
}
export const spot = (t) =>
  t.pool ? t.pool.x / t.pool.y : priceAt(supplyAt(t.raised, t.preset), t.preset);
export const cap = (t) => spot(t) * TOTAL_SUPPLY;
export const virtualLiquidity = (t) => VIRTUAL_SUPPLY * priceAt(0, t.preset) + t.raised;
export { CURVE_SUPPLY };

export function makeToken({
  name,
  ticker,
  art = 'custom',
  preset = PRESETS[0],
  logo = '',
  links = {},
  description = '',
  creator = WALLET,
  contract,
  time = Date.now(),
}) {
  return {
    name,
    ticker,
    art,
    preset,
    description,
    logo,
    links,
    staked: 0,
    creator,
    contract,
    time,
    raised: 0,
    change: 0,
    holders: 0,
    volume: 0,
    verified: false,
    trades: [],
    history: [priceAt(0, preset)],
    owners: {},
  };
}

export const availableTokens = (t) => Math.max(0, (t.owners[WALLET] || 0) - (t.staked || 0));

export function updateStake(t, deposit, amount) {
  const staked = t.staked || 0;
  const limit = deposit ? availableTokens(t) : staked;
  if (!Number.isFinite(amount) || amount <= 0 || amount > limit) return null;
  return { ...t, staked: deposit ? staked + amount : staked - amount };
}

function fillPool(token) {
  return { x: TARGET, y: TARGET / priceAt(CURVE_SUPPLY, token.preset) };
}
export function quote(t, buy, amount, asset) {
  if (!Number.isFinite(amount) || amount <= 0) return null;
  if (asset !== 'XLM') return null;
  const input = amount;
  const supply = supplyAt(t.raised, t.preset);
  if (!t.pool && (buy ? input > TARGET - t.raised + 1e-7 : input > supply)) return null;
  let output;
  if (t.pool) {
    const { x, y } = t.pool;
    output = buy ? (y * input) / (x + input) : (x * input) / (y + input);
  } else {
    output = buy
      ? supplyAt(Math.min(TARGET, t.raised + input), t.preset) - supply
      : t.raised - reserveAt(supply - input, t.preset);
  }
  if (!Number.isFinite(output) || output <= 0) return null;
  const ideal = buy ? input / spot(t) : input * spot(t);
  return {
    input,
    output,
    shown: output,
    impact: Math.max(0, (1 - output / ideal) * 100),
  };
}

export function recordTrade(t, buy, q, account, now = Date.now()) {
  const xlm = buy ? q.input : q.output,
    tokens = buy ? q.output : q.input;
  const sign = buy ? 1 : -1;
  const raised = t.pool ? TARGET : Math.min(TARGET, Math.max(0, t.raised + sign * xlm));
  const pool = t.pool
    ? { x: t.pool.x + sign * xlm, y: t.pool.y - sign * tokens }
    : raised >= TARGET - 1e-7
    ? fillPool(t)
    : null;
  const before = t.owners[account] || 0,
    after = Math.max(0, before + sign * tokens);
  const event = {
    id: crypto.randomUUID(),
    time: now,
    account,
    buy,
    xlm,
    tokens,
    ticker: t.ticker,
    contract: t.contract,
  };
  const next = {
    ...t,
    raised,
    pool,
    owners: { ...t.owners, [account]: after },
    holders: t.holders + Number(after > 1e-7) - Number(before > 1e-7),
    volume: t.volume + xlm,
    trades: [event, ...t.trades].slice(0, 35),
  };
  next.change = (((1 + t.change / 100) * spot(next)) / spot(t) - 1) * 100;
  next.history = [...t.history, spot(next)].slice(-25);
  return next;
}

export function initialMarket() {
  const now = Date.now();
  const tokens = fixtures.map(
    ([name, ticker, art, pct, change, holders, volume, mins, type], i) => {
      const t = {
        ...makeToken({
          name,
          ticker,
          art,
          preset: PRESETS[type],
          ...addresses[i],
          time: now - mins * 60000,
        }),
        raised: (TARGET * pct) / 100,
        change,
        holders,
        volume,
        verified: i % 3 !== 2,
      };
      if (pct === 100) t.pool = fillPool(t);
      const supply = supplyAt(t.raised, t.preset);
      for (let n = 0; n < 6; n++)
        t.owners[addresses[(i + n) % 10].creator] = supply * (0.087 - n * 0.009);
      t.owners[WALLET] = 250 + i * 70;
      t.history = Array.from(
        { length: 24 },
        (_, j) =>
          spot(t) *
          (1 -
            ((change / 140) * (23 - j)) / 23 +
            0.025 * Math.sin(j * 2.4 + i) +
            0.032 * Math.sin(j * 0.7 - i))
      );
      t.history[23] = spot(t);
      t.trades = Array.from({ length: 6 }, (_, j) => {
        const xlm = 80 + ((i * 97 + j * 47) % 860);
        return {
          id: ticker + j,
          time: now - (i * 5 + j * 49 + 3) * 1000,
          account: addresses[(i + j) % 10].creator,
          buy: (i + j) % 3 !== 1,
          xlm,
          tokens: xlm / t.history[23 - j],
          ticker,
          contract: t.contract,
        };
      });
      return t;
    }
  );
  const events = tokens.map((t) => t.trades[0]);
  const dex = tokens.find((t) => t.pool);
  events.splice(2, 0, {
    id: 'first-graduation',
    graduated: true,
    ticker: dex.ticker,
    contract: dex.contract,
    time: now - 18000,
  });
  return { tokens, events, balance: { XLM: 10000 }, connected: false };
}

export function loadMarket() {
  try {
    const saved = JSON.parse(localStorage.getItem(DEMO_KEY));
    if (
      saved?.version === 1 &&
      saved.state?.tokens?.length &&
      saved.state.balance &&
      Array.isArray(saved.state.events)
    )
      return { ...saved.state, balance: { XLM: saved.state.balance.XLM } };
  } catch {
    /* Browser storage is optional. */
  }
  return initialMarket();
}
export function replaceToken(state, previous, next) {
  const events = [next.trades[0], ...state.events];
  if (next.pool && !previous.pool)
    events.unshift({
      id: crypto.randomUUID(),
      graduated: true,
      ticker: next.ticker,
      contract: next.contract,
      time: Date.now(),
    });
  return {
    ...state,
    tokens: state.tokens.map((t) => (t.contract === next.contract ? next : t)),
    events: events.slice(0, 25),
  };
}
export function tick(state) {
  const t = state.tokens[Math.floor(Math.random() * state.tokens.length)];
  const account = addresses[Math.floor(Math.random() * 10)].creator;
  const buy = Math.random() > 0.35 || !t.owners[account];
  const amount = buy
    ? Math.min(28 + Math.random() * 100, t.pool ? Infinity : TARGET - t.raised)
    : Math.min(t.owners[account], (24 + Math.random() * 65) / spot(t));
  const q = quote(t, buy, amount, 'XLM');
  return q ? replaceToken(state, t, recordTrade(t, buy, q, account)) : state;
}
