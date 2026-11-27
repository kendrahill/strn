import React, { useEffect, useMemo, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import logo from './saturn-logo.png';
import { Change, Icon, Meter, TokenCard } from './market-ui.jsx';
import { LaunchDialog, MarketDialog } from './market-dialogs.jsx';
import {
  DEMO_KEY,
  availableTokens,
  updateStake,
  format,
  loadMarket,
  progress,
  quote,
  recordTrade,
  replaceToken,
  short,
  TARGET,
  tick,
  WALLET,
} from './market.js';
import './terminal.css';
import 'virtual:launchpad-utilities.css';

const filters = [
  ['Trending', 'flame'],
  ['New Launches', 'clock-3'],
  ['Graduating Soon', 'trending-up'],
  ['Graduated to DEX', 'graduation-cap'],
];

function Activity({ events, open }) {
  return (
    <div className="activity-strip">
      <div className="bounds activity-inner">
        <span className="live-label">
          <i />
          LIVE FEED
        </span>
        <div className="feed-scroll">
          {events.slice(0, 5).map((e) => (
            <button key={e.id} className="feed-item" onClick={() => open(e.contract)}>
              <Icon
                size={13}
                name={e.graduated ? 'graduation-cap' : e.buy ? 'arrow-down-left' : 'arrow-up-right'}
                className={e.graduated ? 'gold' : e.buy ? 'up' : 'down'}
              />
              <b>${e.ticker}</b>
              <span>{e.graduated ? 'graduated to DEX' : e.buy ? 'buy' : 'sell'}</span>
              {!e.graduated && <strong>{format(e.xlm)} XLM</strong>}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
function Spotlight({ token: t, asset, open }) {
  const pct = progress(t);
  return (
    <section className="spotlight">
      <div className="hero-art" />
      <div className="spotlight-body">
        <div className="king">
          <Icon name="crown" size={15} /> KING OF THE CURVE
        </div>
        <div className="hero-name">
          <h2>{t.name}</h2>
          <span className="hero-symbol font-mono">${t.ticker}</span>
        </div>
        <div className="hero-meta">
          <Change value={t.change} />
          <span>24h</span>
          <i /> <span>Volume</span>
          <strong>
            {format(t.volume)} {asset}
          </strong>
        </div>
        <div className="hero-bottom">
          <div className="hero-progress">
            <div className="flex justify-between items-center">
              <span>Curve progress</span>
              <b>{pct.toFixed(1)}%</b>
            </div>
            <Meter value={pct} smooth />
            <small>
              {format(t.raised)} / {format(TARGET)} {asset} to DEX
            </small>
          </div>
          <button className="quick" onClick={() => open(t.contract, true)}>
            <Icon name="zap" size={15} />
            Quick buy
          </button>
        </div>
      </div>
    </section>
  );
}
function Overview({ tokens, asset }) {
  const volume = tokens.reduce((n, t) => n + t.volume, 0),
    reserve = tokens.reduce((n, t) => n + (t.pool ? t.pool.x : t.raised), 0);
  return (
    <aside className="overview">
      <div className="overview-title flex justify-between items-center">
        <h2>Market overview</h2>
        <Icon name="activity" size={17} />
      </div>
      {[
        [
          'chart-no-axes-column-increasing',
          '24h volume',
          format(volume),
          asset,
          'blue',
        ],
        ['coins', 'Total liquidity', format(reserve), asset, 'violet'],
        ['orbit', 'Active curves', format(tokens.filter((t) => !t.pool).length), '', 'teal'],
      ].map(([icon, label, num, unit, color]) => (
        <div className="overview-row" key={label}>
          <span className={'metric-icon ' + color}>
            <Icon name={icon} size={17} />
          </span>
          <span>{label}</span>
          <strong className="font-mono tabular-nums">
            {num} <small>{unit}</small>
          </strong>
        </div>
      ))}
    </aside>
  );
}
function App() {
  const [state, setState] = useState(loadMarket),
    current = useRef(state);
  const asset = 'XLM';
  const [query, setQuery] = useState(''),
    [filter, setFilter] = useState(0),
    [dialog, setDialog] = useState(null),
    [message, setMessage] = useState('');
  const search = useRef(null);
  function commit(fn) {
    const next = fn(current.current);
    current.current = next;
    setState(next);
  }
  useEffect(() => {
    try {
      localStorage.setItem(DEMO_KEY, JSON.stringify({ version: 1, state }));
    } catch {
      /* Storage can be disabled in private browsing. */
    }
  }, [state]);
  useEffect(() => {
    function key(e) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        if (!document.querySelector('dialog[open]')) {
          search.current.focus();
          search.current.select();
        }
      }
    }
    document.addEventListener('keydown', key);
    const timer = setInterval(() => {
      if (document.visibilityState === 'visible') commit(tick);
    }, 12500);
    return () => {
      document.removeEventListener('keydown', key);
      clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    if (message) {
      const id = setTimeout(() => setMessage(''), 3500);
      return () => clearTimeout(id);
    }
  }, [message]);
  function wallet() {
    const connected = !current.current.connected;
    commit((s) => ({ ...s, connected }));
    setMessage(connected ? 'Demo wallet connected' : 'Demo wallet disconnected');
  }
  function open(contract, quick = false) {
    setDialog({ contract, quick });
  }
  const active = state.tokens.find((t) => t.contract === dialog?.contract);
  const king = state.tokens
    .filter((t) => !t.pool)
    .sort((a, b) => b.raised - a.raised || b.change - a.change)[0];
  const visible = useMemo(() => {
    const q = query.trim().toLowerCase().replace(/^\$/, '');
    const list = state.tokens.filter(
      (t) =>
        (filter === 2 ? !t.pool && progress(t) > 80 : filter === 3 ? !!t.pool : true) &&
        [t.name, t.ticker, t.contract].some((v) => v.toLowerCase().includes(q))
    );
    return list.sort((a, b) =>
      filter === 1 ? b.time - a.time : filter === 2 ? b.raised - a.raised : b.change - a.change
    );
  }, [state.tokens, filter, query]);
  function execute(contract, buy, amount, base, floor) {
    const s = current.current,
      t = s.tokens.find((t) => t.contract === contract),
      q = quote(t, buy, amount, base);
    if (!s.connected || !q || amount > (buy ? s.balance[base] : availableTokens(t)))
      return { message: 'Balance or liquidity changed. Check the new quote.' };
    if (q.output < floor)
      return { message: 'Price moved beyond your slippage tolerance. Review and retry.' };
    const next = recordTrade(t, buy, q, WALLET);
    commit(() => ({
      ...replaceToken(s, t, next),
      balance: { ...s.balance, [base]: Math.max(0, s.balance[base] + (buy ? -amount : q.shown)) },
    }));
    return {
      ok: true,
      message:
        (buy ? 'Bought ' : 'Sold ') +
        format(buy ? q.output : amount, 3) +
        ' ' +
        t.ticker +
        ' · Simulation complete',
    };
  }
  function stake(contract, deposit, amount) {
    const s = current.current;
    const token = s.tokens.find(t => t.contract === contract);
    const updated = s.connected && updateStake(token, deposit, amount);
    if (!updated) return false;
    commit(() => ({ ...s, tokens: s.tokens.map(t => t === token ? updated : t) }));
    return true;
  }
  function create(draft, amount) {
    const s = current.current;
    if (s.tokens.some((t) => t.ticker === draft.ticker))
      return { message: 'That ticker is already in use.' };
    if (
      !s.connected ||
      !Number.isFinite(amount) ||
      amount < 0 ||
      amount > Math.min(TARGET, s.balance.XLM)
    )
      return { message: 'Enter a dev buy within your demo balance and the curve target.' };
    let next = { ...s, tokens: [draft, ...s.tokens] };
    if (amount > 0) {
      const q = quote(draft, true, amount, 'XLM');
      if (!q) return { message: 'Dev buy is too small for a quote.' };
      next = replaceToken(next, draft, recordTrade(draft, true, q, WALLET));
      next.balance = { ...s.balance, XLM: s.balance.XLM - amount };
    }
    commit(() => next);
    setQuery('');
    setFilter(1);
    open(draft.contract);
    setMessage(draft.name + ' launched');
    return { ok: true };
  }
  return (
    <>
      <header className="top-nav">
        <div className="nav-inner bounds">
          <div className="brand-group">
            <a className="brand-link" href="/" aria-label="Saturn home">
              <img src={logo} alt="" />
            </a>
            <span className="nav-divider" />
            <span className="network-pill">
              <span className="network-orb" />
              Stellar <i>/</i> Soroban
            </span>
          </div>
          <div className="search-box">
            <Icon name="search" size={16} />
            <input
              ref={search}
              aria-label="Search tokens by name, ticker, or contract ID"
              type="search"
              placeholder="Search tokens or contract ID"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <kbd>⌘ K</kbd>
          </div>
          <div className="nav-actions">
            <button className="primary" onClick={() => setDialog({ launch: true })}>
              <Icon name="plus" size={16} />
              Launch Token
            </button>
            <button
              className={'wallet-btn ' + (state.connected ? 'connected' : '')}
              onClick={wallet}
              title={
                state.connected ? 'Disconnect demo wallet' : 'Connect simulated Stellar wallet'
              }
              aria-pressed={state.connected}
            >
              <Icon name="wallet" size={16} />
              {state.connected ? (
                <span className="font-mono">
                  {short(WALLET)} <small>{format(state.balance.XLM, 1)} XLM</small>
                </span>
              ) : (
                'Connect wallet'
              )}
            </button>
          </div>
        </div>
      </header>
      <Activity events={state.events} open={open} />
      <main className="bounds terminal-main">
        <div className="page-head">
          <div>
            <h1>
              Launchpad<span className="heading-dot">.</span>
            </h1>
            <p>Launch, trade, and stake tokens on Stellar.</p>
          </div>
          <div className="simulation">
            <span className="small-dot" />
            Simulation online
          </div>
        </div>
        <div className="hero-grid">
          {king ? (
            <Spotlight token={king} asset={asset} open={open} />
          ) : (
            <section className="all-graduated">
              <Icon name="graduation-cap" />
              All curves graduated to DEX.
            </section>
          )}
          <Overview tokens={state.tokens} asset={asset} />
        </div>
        <section className="discovery" aria-labelledby="discovery-title">
          <div className="discovery-header">
            <div className="flex items-center gap-3">
              <Icon name="layout-grid" size={20} />
              <h2 id="discovery-title">Token discovery</h2>
              <span className="count font-mono">{visible.length}</span>
            </div>
            <span className="chip">XLM markets</span>
          </div>
          <div className="filter-tabs" aria-label="Token filters">
            {filters.map(([name, icon], i) => (
              <button
                key={name}
                title={i === 2 ? 'Active curves over 80% funded' : name}
                className={filter === i ? 'selected' : ''}
                aria-pressed={filter === i}
                onClick={() => setFilter(i)}
              >
                <Icon name={icon} size={16} />
                {name}
              </button>
            ))}
          </div>
          <div className="token-grid">
            {visible.map((t) => (
              <TokenCard key={t.contract} token={t} asset={asset} onOpen={() => open(t.contract)} />
            ))}
          </div>
          {!visible.length && (
            <div className="empty">
              <Icon name="search" size={26} />
              <h3>No matching tokens</h3>
              <p>Try another name, ticker, contract ID, or filter.</p>
              <button
                className="wallet-btn"
                onClick={() => {
                  setQuery('');
                  setFilter(0);
                }}
              >
                Reset discovery
              </button>
            </div>
          )}
        </section>
        <footer className="terminal-footer">
          <span>
            <img src={logo} alt="" /> Saturn <i>/</i> Launchpad
          </span>
          <div>
            <a href="/launchpad/testnet/">Testnet tools</a>
            <a href="/docs/">Docs</a>
          </div>
        </footer>
      </main>
      {active && (
        <MarketDialog
          key={active.contract}
          token={active}
          quick={dialog.quick}
          state={state}
          asset={asset}
          connect={wallet}
          execute={execute}
          stake={stake}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.launch && (
        <LaunchDialog
          state={state}
          create={create}
          connect={wallet}
          onClose={() => setDialog(null)}
        />
      )}
      {message && (
        <div className="toast" role="status">
          <Icon name="check" size={15} />
          {message}
        </div>
      )}
    </>
  );
}
createRoot(document.getElementById('root')).render(<App />);
