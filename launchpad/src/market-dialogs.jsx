import React, { useEffect, useId, useRef, useState } from 'react';
import { Avatar, Icon, TokenCard } from './market-ui.jsx';
import {
  age,
  CURVE_SUPPLY,
  format,
  makeToken,
  PRESETS,
  priceAt,
  progress,
  quote,
  short,
  spot,
  supplyAt,
  TARGET,
  units,
  virtualLiquidity,
  WALLET,
} from './market.js';

export function Dialog({ children, onClose, className = '' }) {
  const ref = useRef(null);
  useEffect(() => {
    const node = ref.current,
      old = document.body.style.overflow;
    node.showModal();
    document.body.style.overflow = 'hidden';
    return () => {
      node.close();
      document.body.style.overflow = old;
    };
  }, []);
  return (
    <dialog
      ref={ref}
      aria-labelledby="dialog-title"
      className={'dialog ' + className}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          const r = e.currentTarget.getBoundingClientRect();
          if (
            e.clientX < r.left ||
            e.clientX > r.right ||
            e.clientY < r.top ||
            e.clientY > r.bottom
          )
            onClose();
        }
      }}
    >
      {children}
    </dialog>
  );
}
function Close({ onClose }) {
  return (
    <button type="button" className="icon-btn" aria-label="Close dialog" onClick={onClose}>
      <Icon name="x" size={20} />
    </button>
  );
}
function Chart({ token: t }) {
  const [cursor, setCursor] = useState(null),
    id = useId().replace(/:/g, '');
  const sold = supplyAt(t.raised, t.preset) / CURVE_SUPPLY,
    selected = cursor ?? sold;
  const x = (v) => 44 + v * 406,
    y = (v) => 150 - (122 * priceAt(CURVE_SUPPLY * v, t.preset)) / priceAt(CURVE_SUPPLY, t.preset);
  const points = Array.from({ length: 81 }, (_, i) => [x(i / 80), y(i / 80)].join(',')).join(' ');
  return (
    <div className="chart-box">
      <div className="flex justify-between items-center">
        <h3>Bonding curve</h3>
        <span className="chip">{t.preset}</span>
      </div>
      <div className="chart-caption">
        <span className="legend-dot" />
        {t.pool ? 'Final curve price' : 'Spot'}
        <b className="font-mono">{format(priceAt(sold * CURVE_SUPPLY, t.preset), 6)} XLM</b>
      </div>
      <svg
        className="curve-chart"
        viewBox="0 0 478 186"
        role="slider"
        tabIndex="0"
        aria-label="Inspect curve by token supply"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={Math.round(selected * 100)}
        aria-valuetext={
          format(selected * CURVE_SUPPLY) +
          ' tokens, ' +
          format(priceAt(selected * CURVE_SUPPLY, t.preset), 6) +
          ' XLM'
        }
        onPointerMove={(e) => {
          const svg = e.currentTarget,
            point = svg.createSVGPoint();
          point.x = e.clientX;
          point.y = e.clientY;
          const local = point.matrixTransform(svg.getScreenCTM().inverse());
          setCursor(Math.max(0, Math.min(1, (local.x - 44) / 406)));
        }}
        onPointerLeave={() => setCursor(null)}
        onBlur={() => setCursor(null)}
        onKeyDown={(e) => {
          if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) {
            e.preventDefault();
            setCursor(
              e.key === 'Home'
                ? 0
                : e.key === 'End'
                ? 1
                : Math.max(0, Math.min(1, selected + (e.key === 'ArrowLeft' ? -0.01 : 0.01)))
            );
          }
        }}
      >
        <defs>
          <linearGradient id={id} x2="0" y2="1">
            <stop stopColor="var(--cyan)" stopOpacity=".3" />
            <stop offset="1" stopColor="var(--cyan)" stopOpacity=".01" />
          </linearGradient>
        </defs>
        {[0, 1, 2, 3].map((k) => (
          <g key={k}>
            <line className="gridline" x1="44" x2="450" y1={150 - k * 40.67} y2={150 - k * 40.67} />
            <text x="0" y={153 - k * 40.67}>
              {((priceAt(CURVE_SUPPLY, t.preset) * k) / 3).toFixed(3)}
            </text>
          </g>
        ))}
        {[0, 0.25, 0.5, 0.75, 1].map((v) => (
          <g key={v}>
            <line className="gridline" x1={x(v)} x2={x(v)} y1="25" y2="150" />
            <text x={x(v)} y="171" textAnchor="middle">
              {v === 1 ? '1M' : v === 0 ? '0' : v * 1000 + 'K'}
            </text>
          </g>
        ))}
        <polygon points={'44,150 ' + points + ' 450,150'} fill={'url(#' + id + ')'} />
        <polyline points={points} fill="none" stroke="var(--cyan)" strokeWidth="2.6" />
        <line x1="450" x2="450" y1="23" y2="150" stroke="var(--gold)" strokeDasharray="3 4" />
        <text className="goal-label" x="450" y="16" textAnchor="end">
          DEX target
        </text>
        <circle cx={x(sold)} cy={y(sold)} r="7" fill="var(--cyan)" opacity=".24" />
        <circle cx={x(sold)} cy={y(sold)} r="3.5" fill="var(--cyan)" />
        {cursor !== null && (
          <g>
            <line
              x1={x(cursor)}
              x2={x(cursor)}
              y1="26"
              y2="150"
              stroke="var(--muted)"
              strokeDasharray="2 4"
            />
            <circle cx={x(cursor)} cy={y(cursor)} r="4" fill="var(--text)" />
          </g>
        )}
      </svg>
      <div className="chart-readout flex justify-between">
        <span>
          Supply <b>{format(selected * CURVE_SUPPLY)}</b>
        </span>
        <span>
          Price <b>{format(priceAt(selected * CURVE_SUPPLY, t.preset), 6)} XLM</b>
        </span>
      </div>
      <dl className="curve-stats">
        <div>
          <dt>{t.pool ? 'DEX reserve' : 'Virtual reserve'}</dt>
          <dd>{format(t.pool ? t.pool.x : virtualLiquidity(t))} XLM</dd>
        </div>
        <div>
          <dt>{t.pool ? 'Pool tokens' : 'Virtual tokens'}</dt>
          <dd>{format(t.pool ? t.pool.y : virtualLiquidity(t) / spot(t))} {t.ticker}</dd>
        </div>
        <div>
          <dt>Raised / target</dt>
          <dd>{format(t.raised)} / {format(TARGET)} XLM</dd>
        </div>
      </dl>
    </div>
  );
}
function Ledger({ token: t }) {
  const [tab, setTab] = useState('Recent trades');
  const owners = Object.entries(t.owners)
    .filter(([, v]) => v > 0)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 6);
  return (
    <section className="ledger">
      <div className="small-tabs">
        {['Recent trades', 'Top holders'].map((s) => (
          <button key={s} aria-pressed={s === tab} onClick={() => setTab(s)}>
            {s}
          </button>
        ))}
      </div>
      {tab === 'Recent trades' ? (
        <table>
          <thead>
            <tr>
              <th>Account</th>
              <th>Side</th>
              <th>Volume</th>
              <th>Time</th>
            </tr>
          </thead>
          <tbody>
            {t.trades.slice(0, 5).map((e) => (
              <tr key={e.id}>
                <td>{short(e.account)}</td>
                <td className={e.buy ? 'up' : 'down'}>{e.buy ? 'Buy' : 'Sell'}</td>
                <td>{format(e.xlm, 2)} XLM</td>
                <td>{age(e.time)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Rank</th>
              <th>Wallet</th>
              <th>Balance ({t.ticker})</th>
            </tr>
          </thead>
          <tbody>
            {owners.map(([a, v], i) => (
              <tr key={a}>
                <td>{i + 1}</td>
                <td>{short(a)}</td>
                <td>{format(v, 2)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {!(tab === 'Recent trades' ? t.trades.length : owners.length) && (
        <p className="empty-table">Activity will appear after the first trade.</p>
      )}
    </section>
  );
}

function Swap({ token: t, state, asset: initialAsset, connect, execute, quick }) {
  const [buy, setBuy] = useState(true),
    [asset, setAsset] = useState(initialAsset),
    [amount, setAmount] = useState(() =>
      quick
        ? String(
            Math.floor(
              Math.min(
                100,
                state.balance[initialAsset],
                t.pool ? Infinity : units(TARGET - t.raised, initialAsset)
              ) * 1e7
            ) / 1e7
          )
        : ''
    );
  const [slip, setSlip] = useState(0.5),
    [settings, setSettings] = useState(false),
    [busy, setBusy] = useState(false),
    [result, setResult] = useState('');
  const timer = useRef(null);
  useEffect(() => () => clearTimeout(timer.current), []);
  const available = buy ? state.balance[asset] : t.owners[WALLET] || 0,
    input = Number(amount),
    inputUnit = buy ? asset : t.ticker;
  const max = buy && !t.pool ? Math.min(available, units(TARGET - t.raised, asset)) : available;
  const q = quote(t, buy, input, asset);
  const error =
    amount && input > 0
      ? input > available
        ? 'Insufficient demo balance.'
        : !q
        ? 'Amount exceeds available curve liquidity.'
        : ''
      : '';
  function submit(e) {
    e.preventDefault();
    setResult('');
    if (!state.connected) {
      connect();
      return;
    }
    if (!q || error || busy) return;
    setBusy(true);
    timer.current = setTimeout(() => {
      const outcome = execute(t.contract, buy, input, asset, q.output * (1 - slip / 100));
      setBusy(false);
      setResult(outcome.message);
      if (outcome.ok) setAmount('');
    }, 650);
  }
  function side(v) {
    setBuy(v);
    setAmount('');
    setResult('');
  }
  return (
    <form className="swap" onSubmit={submit}>
      <div className="flex justify-between items-center">
        <h3>Trade {t.ticker}</h3>
        <button
          type="button"
          className="icon-btn"
          aria-label="Adjust slippage"
          aria-expanded={settings}
          onClick={() => setSettings(!settings)}
        >
          <Icon name="settings-2" />
        </button>
      </div>
      <fieldset disabled={busy}>
        <div className="side-switch">
          {[true, false].map((v) => (
            <button
              type="button"
              key={String(v)}
              className={buy === v ? (v ? 'buy-active' : 'sell-active') : ''}
              aria-pressed={buy === v}
              onClick={() => side(v)}
            >
              {v ? 'Buy' : 'Sell'}
            </button>
          ))}
        </div>
        <label htmlFor="trade-asset">Reserve asset</label>
        <select
          id="trade-asset"
          value={asset}
          onChange={(e) => {
            setAsset(e.target.value);
            setAmount('');
            setResult('');
          }}
        >
          <option>XLM</option>
          <option>USDC</option>
        </select>
        <label htmlFor="trade-amount">You pay</label>
        <div className="amount-field">
          <input
            id="trade-amount"
            type="number"
            min="0"
            step="any"
            inputMode="decimal"
            placeholder="0.00"
            value={amount}
            onChange={(e) => {
              setAmount(e.target.value);
              setResult('');
            }}
          />
          <b>{inputUnit}</b>
        </div>
        <div className="presets">
          {[100, 500, 1000, 'MAX'].map((v) => (
            <button
              type="button"
              key={v}
              onClick={() => {
                setAmount(String(v === 'MAX' ? Math.floor(max * 1e7) / 1e7 : v));
                setResult('');
              }}
            >
              {v === 'MAX' ? v : format(v) + ' ' + inputUnit}
            </button>
          ))}
        </div>
        <div className="balance">
          Available{' '}
          <b>
            {format(available, 2)} {inputUnit}
          </b>
        </div>
        <div className="receive">
          <span className="label">Estimated receive</span>
          <div className="font-mono">
            {format(q?.shown || 0, 4)} <small>{buy ? t.ticker : asset}</small>
          </div>
        </div>
        <div className="quote-row">
          <span>Price impact</span>
          <b>{q ? q.impact.toFixed(2) + '%' : '—'}</b>
        </div>
        <div className="quote-row">
          <span>Slippage tolerance</span>
          <button type="button" onClick={() => setSettings(!settings)}>
            {slip}% <Icon name="chevron-down" size={12} />
          </button>
        </div>
        {settings && (
          <div className="presets">
            {[0.1, 0.5, 1, 3].map((v) => (
              <button type="button" key={v} aria-pressed={v === slip} onClick={() => setSlip(v)}>
                {v}%
              </button>
            ))}
          </div>
        )}
        <button
          className="primary swap-submit"
          disabled={state.connected && (!q || !!error || busy)}
        >
          {busy
            ? 'Simulating trade…'
            : !state.connected
            ? 'Connect demo wallet'
            : (buy ? 'Buy ' : 'Sell ') + t.ticker}
        </button>
      </fieldset>
      <p className="form-status" role="status">
        {error || result || 'Simulated trade · Demo balances only'}
      </p>
    </form>
  );
}
export function MarketDialog({ token: t, onClose, ...props }) {
  const [copied, setCopied] = useState(false);
  return (
    <Dialog onClose={onClose} className="market-dialog">
      <header className="dialog-head">
        <div className="flex items-center gap-3">
          <Avatar token={t} />
          <div>
            <h2 id="dialog-title">{t.name}</h2>
            <span className="ticker font-mono">${t.ticker}</span>
          </div>
        </div>
        <Close onClose={onClose} />
      </header>
      <div className="contract-line">
        <span>{t.pool ? 'Graduated to DEX' : progress(t).toFixed(1) + '% to graduation'}</span>
        <details>
          <summary>Contract {short(t.contract)}</summary>
          <div>
            <code>{t.contract}</code>
            <button
              className="icon-btn"
              type="button"
              aria-label="Copy contract ID"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(t.contract);
                  setCopied(true);
                } catch {
                  setCopied(false);
                }
              }}
            >
              <Icon name={copied ? 'check' : 'copy'} size={14} />
            </button>
          </div>
        </details>
      </div>
      {t.description && <p className="description">{t.description}</p>}
      <div className="detail-grid">
        <div>
          <Chart token={t} />
          <Ledger token={t} />
        </div>
        <Swap token={t} {...props} />
      </div>
    </Dialog>
  );
}

export function LaunchDialog({ onClose, state, create, connect }) {
  const [name, setName] = useState(''),
    [ticker, setTicker] = useState(''),
    [description, setDescription] = useState(''),
    [preset, setPreset] = useState(PRESETS[0]),
    [initial, setInitial] = useState(''),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false);
  const amount = Number(initial) || 0;
  const draft = {
    ...makeToken({
      name: name.trim() || 'Your token',
      ticker: ticker || 'TOKEN',
      description: description.trim(),
      preset,
    }),
    raised: Math.max(0, Math.min(TARGET, amount)),
    holders: amount > 0 ? 1 : 0,
  };
  async function submit(e) {
    e.preventDefault();
    setError('');
    if (!state.connected) {
      connect();
      return;
    }
    if (!name.trim() || !ticker || !description.trim()) {
      setError('Enter a name, ticker, and description.');
      return;
    }
    setBusy(true);
    try {
      const { StrKey } = await import('@stellar/stellar-sdk');
      const contract = StrKey.encodeContract(crypto.getRandomValues(new Uint8Array(32)));
      const outcome = create({ ...draft, contract, raised: 0, holders: 0 }, amount);
      if (!outcome.ok) setError(outcome.message);
    } catch {
      setError('Could not create this demo token. Try again.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog onClose={onClose} className="launch-dialog">
      <header className="dialog-head">
        <div>
          <div className="eyeline">
            <Icon name="orbit" size={14} /> STELLAR / SOROBAN
          </div>
          <h2 id="dialog-title">Launch a token</h2>
        </div>
        <Close onClose={onClose} />
      </header>
      <div className="launch-grid">
        <form onSubmit={submit}>
          <div className="name-row">
            <label>
              Token name
              <input
                type="text"
                required
                maxLength={36}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Aurora Protocol"
              />
            </label>
            <label>
              Ticker
              <input
                type="text"
                required
                pattern="[A-Z][A-Z0-9]{1,9}"
                title="2–10 characters, starting with a letter"
                maxLength={10}
                value={ticker}
                onChange={(e) => setTicker(e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ''))}
                placeholder="AURA"
              />
            </label>
          </div>
          <label>
            Description
            <textarea
              required
              maxLength={280}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What is your token for?"
              rows={3}
            />
          </label>
          <label>
            Curve preset
            <select value={preset} onChange={(e) => setPreset(e.target.value)}>
              {PRESETS.map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          </label>
          <label>
            Initial dev buy <span className="muted">(optional)</span>
            <div className="amount-field">
              <input
                type="number"
                min="0"
                max={Math.min(state.balance.XLM, TARGET)}
                step="0.0000001"
                inputMode="decimal"
                value={initial}
                onChange={(e) => setInitial(e.target.value)}
                placeholder="0.00"
              />
              <b>XLM</b>
            </div>
          </label>
          <button disabled={busy} className="primary swap-submit">
            <Icon name="plus" />
            {busy ? 'Creating…' : state.connected ? 'Launch token' : 'Connect demo wallet'}
          </button>
          <p className="form-status" role="status">
            {error || 'This launch uses your demo wallet.'}
          </p>
        </form>
        <aside className="preview">
          <div className="eyeline">
            <span className="small-dot" /> LIVE PREVIEW
          </div>
          <TokenCard token={draft} />
          {draft.description && <p>{draft.description}</p>}
        </aside>
      </div>
    </Dialog>
  );
}
