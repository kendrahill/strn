import React, { useId } from 'react';
import sprite from './terminal-icons.svg?no-inline';
import { age, cap, format, progress, short, TARGET, XLM_USD } from './market.js';

export function Icon({ name, size = 17, className = '' }) {
  return (
    <svg
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={'icon ' + className}
    >
      <use href={sprite + '#' + name} />
    </svg>
  );
}

const drawings = {
  aurora: (
    <>
      <path d="m5 44 20-33 8 14 5-9 19 30-19-15-5 7-8-13-10 15Z" fill="var(--art-light)" />
      <path d="m9 47 17-16 7 12 6-7 12 11-12-5-7 8-7-10Z" fill="var(--art-accent)" />
    </>
  ),
  nova: (
    <path
      d="m32 5 4 19 15-9-9 15 17 3-19 5 8 13-14-10-4 18-4-19-15 8 10-14-17-4 19-3-7-15 13 11Z"
      fill="var(--art-light)"
    />
  ),
  orbital: (
    <>
      <circle cx="32" cy="32" r="15" fill="var(--art-light)" />
      <ellipse
        cx="32"
        cy="32"
        rx="28"
        ry="8"
        transform="rotate(-32 32 32)"
        stroke="var(--art-accent)"
        strokeWidth="4"
        fill="none"
      />
      <path d="M18 31a14 14 0 0 1 18-12" stroke="var(--art-light)" strokeWidth="3" fill="none" />
    </>
  ),
  prism: (
    <>
      <path d="m31 6 23 44-42-5Z" fill="none" stroke="var(--art-light)" strokeWidth="3" />
      <path d="m31 6 4 27 19 17-23-8-19 3 23-12Z" fill="var(--art-accent)" />
      <path d="m31 6 4 27-23 12Z" fill="var(--art-light)" opacity=".38" />
    </>
  ),
  helios: (
    <>
      {Array.from({ length: 14 }, (_, i) => (
        <path
          key={i}
          d="m30 6 4 8"
          stroke="var(--art-light)"
          strokeWidth="3.5"
          transform={'rotate(' + (i * 360) / 14 + ' 32 32)'}
        />
      ))}
      <circle cx="32" cy="32" r="13" fill="var(--art-light)" />
    </>
  ),
  vortex: (
    <>
      {Array.from({ length: 5 }, (_, i) => (
        <path
          key={i}
          d="M32 32Q53 28 46 8Q63 30 38 37Z"
          fill="var(--art-light)"
          transform={'rotate(' + 72 * i + ' 32 32)'}
        />
      ))}
    </>
  ),
  lunar: (
    <>
      <path d="M44 10a23 23 0 1 0 9 33A24 24 0 0 1 44 10Z" fill="var(--art-light)" />
      <path d="m47 16 2 6 6 2-6 2-2 6-2-6-6-2 6-2Z" fill="var(--art-accent)" />
    </>
  ),
  cobalt: (
    <>
      <path d="m10 22 22-13 22 13-22 14Z" fill="var(--art-light)" />
      <path d="m10 26 20 13v17L10 44Z" fill="var(--art-accent)" />
      <path d="M34 39 54 26v18L34 56Z" fill="var(--art-light)" opacity=".7" />
    </>
  ),
  vector: (
    <>
      <path d="m5 16 27 36 27-36-27 20Z" fill="var(--art-light)" />
      <path d="m14 11 18 19 18-19-18 10Z" fill="var(--art-accent)" />
    </>
  ),
};
export function Avatar({ token }) {
  return (
    <span className={'avatar art-' + token.art} aria-hidden="true">
      {token.logo ? <img src={token.logo} alt="" /> : drawings[token.art] ? (
        <svg viewBox="0 0 64 64">{drawings[token.art]}</svg>
      ) : (
        token.ticker.slice(0, 2)
      )}
    </span>
  );
}
export function Sparkline({ data, positive = true }) {
  const id = useId().replace(/:/g, '');
  const min = Math.min(...data),
    max = Math.max(...data),
    range = max - min || 1;
  const points = data.map((v, i) => [
    (i * 118) / Math.max(1, data.length - 1),
    30 - ((v - min) / range) * 24,
  ]);
  const line = points.map((p) => p.join(',')).join(' ');
  return (
    <svg
      aria-hidden="true"
      className={'spark ' + (positive ? 'up' : 'down')}
      viewBox="0 0 120 37"
      preserveAspectRatio="none"
    >
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop stopColor="currentColor" stopOpacity=".24" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={'0,37 ' + line + ' 118,37'} fill={'url(#' + id + ')'} />
      <polyline
        points={line}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.65"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
export function Change({ value }) {
  return (
    <span className={'change font-mono tabular-nums ' + (value >= 0 ? 'up' : 'down')}>
      {value >= 0 ? '↗' : '↘'} {value >= 0 ? '+' : ''}
      {value.toFixed(2)}%
    </span>
  );
}
export function Meter({ value, smooth = false }) {
  return (
    <div
      className={'meter ' + (smooth ? 'smooth' : 'segmented')}
      role="meter"
      aria-label="Bonding curve progress"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={Number(value.toFixed(1))}
    >
      <i style={{ width: value + '%' }} />
    </div>
  );
}
export function TokenCard({ token: t, asset = 'XLM', onOpen }) {
  const pct = progress(t);
  const content = (
    <>
      <div className="flex items-start justify-between gap-2">
        <div className="token-id flex items-center gap-3">
          <Avatar token={t} />
          <div>
            <h3>{t.name}</h3>
            <span className="ticker font-mono">${t.ticker}</span>
          </div>
        </div>
        {t.pool ? (
          <span className="badge graduated">
            <Icon name="graduation-cap" size={13} />
            Graduated
          </span>
        ) : pct > 80 ? (
          <span className="badge soon">
            <i />
            Graduating soon
          </span>
        ) : t.verified ? (
          <span className="verified" title="Verified contract" aria-label="Verified contract">
            <Icon name="badge-check" />
          </span>
        ) : null}
      </div>
      <div className="meta flex items-center gap-2">
        <span className="font-mono" title={'Creator: ' + t.creator}>{short(t.creator)}</span>
        <b>·</b>
        <span>{age(t.time)}</span>
      </div>
      <div className="cap-row flex items-end justify-between">
        <div>
          <div className="label">Market cap</div>
          <strong className="font-mono tabular-nums">
            {format(cap(t))} <small>{asset}</small>
          </strong>
          <div className="usd font-mono">
            ${format(cap(t) * XLM_USD)} USD
          </div>
        </div>
        <div className="trend">
          <Sparkline data={t.history} positive={t.change >= 0} />
          <Change value={t.change} />
        </div>
      </div>
      <div className="card-curve">
        <div className="flex justify-between items-center">
          <span className="label">Curve progress</span>
          <strong className={'pct font-mono ' + (t.pool ? 'up' : '')}>{pct.toFixed(1)}%</strong>
        </div>
        <Meter value={pct} />
        <div className="card-bottom flex justify-between items-center gap-2">
          <span className="font-mono">
            {format(t.raised)} / {format(TARGET)} {asset}
            {t.pool ? '' : ' to DEX'}
          </span>
          <span className="flex items-center gap-1" title={format(t.holders) + ' holders'}>
            <Icon name="users" size={13} />
            <span className="font-mono">{format(t.holders)}</span>
          </span>
        </div>
      </div>
    </>
  );
  return onOpen ? (
    <button
      className="token-card text-left"
      onClick={onOpen}
      aria-label={'View ' + t.name + ' market'}
    >
      {content}
    </button>
  ) : (
    <article className="token-card">{content}</article>
  );
}
