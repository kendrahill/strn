import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CURVE_SUPPLY, makeToken, PRESETS, priceAt, progress, quote, recordTrade,
  replaceToken, spot, TARGET, WALLET, availableTokens, updateStake,
} from './market.js';

for (const preset of PRESETS) {
  test(preset + ' quotes reverse in XLM', () => {
    const initial = makeToken({ name: 'Test token', ticker: 'TEST', preset });
    assert.equal(quote(initial, true, 120, 'USDC'), null);
    const buy = quote(initial, true, 120, 'XLM');
    const funded = recordTrade(initial, true, buy, WALLET);
    assert.equal(funded.raised, 120);
    assert.equal(funded.holders, 1);
    assert.ok(spot(funded) > spot(initial));
    assert.ok(buy.impact > 0);
    const sell = quote(funded, false, buy.output, 'XLM');
    assert.ok(Math.abs(sell.shown - funded.raised) < 1e-6);
    const empty = recordTrade(funded, false, sell, WALLET);
    assert.equal(empty.holders, 0);
    assert.ok(empty.raised < 1e-6);
    assert.equal(quote(empty, false, 100, 'XLM'), null);
  });

  test(preset + ' graduates at the target and preserves the pool invariant', () => {
    const initial = makeToken({ name: 'Test token', ticker: 'TEST', preset, contract: 'fixture' });
    assert.equal(quote(initial, true, TARGET + 1, 'XLM'), null);
    const graduated = recordTrade(initial, true, quote(initial, true, TARGET, 'XLM'), WALLET);
    assert.equal(progress(graduated), 100);
    assert.ok(Math.abs(spot(graduated) - priceAt(CURVE_SUPPLY, preset)) < 1e-10);
    const state = replaceToken({ tokens: [initial], events: [] }, initial, graduated);
    assert.ok(state.events[0].graduated);
    const swapped = recordTrade(graduated, true, quote(graduated, true, 100, 'XLM'), WALLET);
    const k = graduated.pool.x * graduated.pool.y;
    assert.ok(Math.abs(swapped.pool.x * swapped.pool.y / k - 1) < 1e-12);
    assert.equal(replaceToken(state, graduated, swapped).events.filter(e => e.graduated).length, 1);
  });
}

test('stake reserves holdings until withdrawn, including older saved tokens', () => {
  const token = { owners: { [WALLET]: 30 } };
  const position = updateStake(token, true, 20);
  assert.equal(availableTokens(position), 10);
  assert.equal(updateStake(position, true, 11), null);
  assert.equal(updateStake(position, false, 21), null);
  assert.equal(availableTokens(updateStake(position, false, 20)), 30);
});
