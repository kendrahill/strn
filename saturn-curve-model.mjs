import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

// Amounts use atomic token units or XLM stroops. Prices use stroops per whole token.
export const UNIT = 10_000_000n;
export const SALE_INVENTORY = 400_000_000n * UNIT;
export const SEED_INVENTORY = 300_000_000n * UNIT;
export const INITIAL_GRADUATION_TARGET = 30_000n * UNIT;
export const MAX_GRADUATION_TARGET = 1_000_000n * UNIT;
const FEE_BPS = 100n;
export const GRADUATION_FEE_BPS = 50n;
export const INITIAL_CREATION_FEE = 100n * UNIT;
export const MAX_CREATION_FEE = 1_000n * UNIT;
const BPS = 10_000n;

function units(amount, label) {
  if (typeof amount !== 'bigint' || amount < 0n) {
    throw new RangeError(label + ' must be a nonnegative bigint');
  }
  return amount;
}

function validTarget(target) {
  if (units(target, 'Graduation target') < UNIT || target > MAX_GRADUATION_TARGET) {
    throw new RangeError('Graduation target is out of range');
  }
  return target;
}

export function reserveAt(sold, target = INITIAL_GRADUATION_TARGET) {
  if (units(sold, 'Sold inventory') > SALE_INVENTORY) {
    throw new RangeError('Sold inventory exceeds the allocation');
  }
  return validTarget(target) * sold * (2n * SALE_INVENTORY + sold) /
    (3n * SALE_INVENTORY * SALE_INVENTORY);
}

function feeLeg(principal) {
  return principal * FEE_BPS / (2n * BPS);
}

function quoteCost(sold, quantity, target) {
  const principal = reserveAt(sold + quantity, target) - reserveAt(sold, target);
  return principal + feeLeg(principal);
}

function lastAffordable(low, high, fits) {
  while (low < high) {
    const mid = (low + high + 1n) / 2n;
    if (fits(mid)) low = mid;
    else high = mid - 1n;
  }
  return low;
}

// The reference assumes at least one atomic unit on each positive fee leg.
// A template with different fees or minimums must recompute this boundary.
const targetReserve = INITIAL_GRADUATION_TARGET;
function isDust(sold, target) {
  return feeLeg(SALE_INVENTORY - sold) === 0n ||
    feeLeg(target - reserveAt(sold, target)) === 0n;
}

export function dustCap(target = INITIAL_GRADUATION_TARGET) {
  return lastAffordable(0n, SALE_INVENTORY, (tail) => isDust(SALE_INVENTORY - tail, target));
}

export const TERMINAL_LOT = dustCap() + 1n;
export const GRADUATION_DUST_CAP = TERMINAL_LOT - 1n;

export function initialState(target = INITIAL_GRADUATION_TARGET) {
  return Object.freeze({ sold: 0n, feeCursor: 0, graduated: false, target: validTarget(target) });
}

export const INITIAL_STATE = initialState();

export function quoteCreation(maxFee, governanceFee = INITIAL_CREATION_FEE) {
  units(maxFee, 'Authorized creation fee');
  if (units(governanceFee, 'Governance creation fee') > MAX_CREATION_FEE) {
    throw new RangeError('Creation fee exceeds governance limit');
  }
  if (governanceFee > maxFee) throw new RangeError('Creation fee exceeds authorization');
  return { operationTreasuryXlm: governanceFee };
}

function requireActive({ sold, feeCursor, graduated, target = INITIAL_GRADUATION_TARGET }) {
  reserveAt(sold, target);
  if (graduated || sold === SALE_INVENTORY) throw new RangeError('Curve has graduated');
  if (isDust(sold, target)) throw new RangeError('Invalid inventory tail');
  if (!Number.isInteger(feeCursor) || feeCursor < 0 || feeCursor > 3) {
    throw new RangeError('Invalid XLM fee cursor');
  }
}

function fees(tokenPrincipal, xlmPrincipal, cursor) {
  const burn = feeLeg(tokenPrincipal);
  const xlmFee = feeLeg(xlmPrincipal);
  if (burn === 0n || xlmFee === 0n) throw new RangeError('Trade below the fee minimum');
  const quarter = xlmFee / 4n;
  const xlm = { creator: quarter, foundation: quarter, strnStakers: quarter * 2n };
  const recipients = ['strnStakers', 'creator', 'strnStakers', 'foundation'];
  for (let residue = Number(xlmFee % 4n); residue > 0; residue -= 1) {
    xlm[recipients[cursor]] += 1n;
    cursor = (cursor + 1) % recipients.length;
  }
  return { burn, xlmFee, xlm, nextFeeCursor: cursor };
}

export function quoteBuy(state, { maxXlm, minTokens, allowUnspent = false }) {
  requireActive(state);
  const target = state.target ?? INITIAL_GRADUATION_TARGET;
  units(maxXlm, 'XLM budget');
  units(minTokens, 'Minimum token output');
  const remaining = SALE_INVENTORY - state.sold;
  const tokens = lastAffordable(0n, remaining,
    (quantity) => quoteCost(state.sold, quantity, target) <= maxXlm);

  const sold = state.sold + tokens;
  const inventoryLeft = SALE_INVENTORY - sold;
  const terminal = isDust(sold, target);
  const xlmPrincipal = reserveAt(sold, target) - reserveAt(state.sold, target);
  const settlement = fees(tokens, xlmPrincipal, state.feeCursor);
  const xlmIn = xlmPrincipal + settlement.xlmFee;
  const tokensOut = tokens - settlement.burn;
  if (tokensOut < minTokens) throw new RangeError('Minimum token output not met');
  if (xlmIn < maxXlm && !allowUnspent) throw new RangeError('Unspent XLM requires consent');

  const fundedReserve = reserveAt(sold, target);
  const graduationFee = fundedReserve * GRADUATION_FEE_BPS / BPS;
  // The final fill has only a graduated result. Contract execution must commit
  // all custody changes together or leave the previous state in place.
  return {
    state: { sold, feeCursor: settlement.nextFeeCursor, graduated: terminal, target },
    tokens, xlmPrincipal, xlmIn, tokensOut, unspentXlm: maxXlm - xlmIn,
    burn: settlement.burn, xlmFees: settlement.xlm,
    curveReserve: terminal ? 0n : fundedReserve,
    graduation: terminal
      ? {
          dustBurn: inventoryLeft, grossXlmReserve: fundedReserve,
          operationTreasuryXlm: graduationFee, xlmReserve: fundedReserve - graduationFee,
          tokenReserve: SEED_INVENTORY, lockedGenesisShares: true
        }
      : null
  };
}

export function quoteSell(state, { tokens, maxTokens, minXlm }) {
  requireActive(state);
  const target = state.target ?? INITIAL_GRADUATION_TARGET;
  units(tokens, 'Token principal');
  units(maxTokens, 'Token debit limit');
  units(minXlm, 'Minimum XLM output');
  if (tokens > state.sold) throw new RangeError('Redemption exceeds net curve sales');

  const sold = state.sold - tokens;
  const xlmPrincipal = reserveAt(state.sold, target) - reserveAt(sold, target);
  const settlement = fees(tokens, xlmPrincipal, state.feeCursor);
  const tokensIn = tokens + settlement.burn;
  const xlmOut = xlmPrincipal - settlement.xlmFee;
  if (tokensIn > maxTokens) throw new RangeError('Token debit exceeds authorization');
  if (xlmOut < minXlm) throw new RangeError('Minimum XLM output not met');

  return {
    state: { sold, feeCursor: settlement.nextFeeCursor, graduated: false, target },
    tokens, xlmPrincipal, tokensIn, xlmOut,
    burn: settlement.burn, xlmFees: settlement.xlm, curveReserve: reserveAt(sold, target)
  };
}

function checkModel() {
  const half = SALE_INVENTORY / 2n;
  assert.equal(reserveAt(0n), 0n);
  assert.equal(reserveAt(half), 12_500n * UNIT);
  assert.equal(targetReserve, 30_000n * UNIT);
  assert.equal(targetReserve * UNIT, 1_000n * SEED_INVENTORY);
  const lowered = 15_000n * UNIT;
  const newLaunch = initialState(lowered);
  assert.equal(reserveAt(SALE_INVENTORY, lowered), lowered);
  assert.equal(reserveAt(half, lowered), 6_250n * UNIT);
  assert.ok(dustCap(lowered) > GRADUATION_DUST_CAP);
  const lowerClosing = quoteBuy(newLaunch, { maxXlm: quoteCost(0n, SALE_INVENTORY, lowered), minTokens: 1n });
  assert.equal(lowerClosing.graduation.operationTreasuryXlm, 75n * UNIT);
  assert.equal(lowerClosing.graduation.xlmReserve, 14_925n * UNIT);
  assert.equal(INITIAL_STATE.target, INITIAL_GRADUATION_TARGET);
  assert.equal(TERMINAL_LOT, 1_990_001n);
  assert.equal(GRADUATION_DUST_CAP, 1_990_000n);
  assert.deepEqual(quoteCreation(100n * UNIT), { operationTreasuryXlm: 100n * UNIT });
  assert.deepEqual(quoteCreation(125n * UNIT, 125n * UNIT), { operationTreasuryXlm: 125n * UNIT });
  assert.throws(() => quoteCreation(100n * UNIT, 125n * UNIT), /authorization/);
  assert.throws(() => quoteCreation(MAX_CREATION_FEE + 1n, MAX_CREATION_FEE + 1n), /limit/);

  const buyHalf = quoteBuy(INITIAL_STATE, { maxXlm: quoteCost(0n, half), minTokens: 1n });
  const sellHalf = quoteSell(buyHalf.state, {
    tokens: buyHalf.tokens, maxTokens: buyHalf.tokens + feeLeg(buyHalf.tokens), minXlm: 1n
  });
  assert.equal(buyHalf.curveReserve, 12_500n * UNIT);
  assert.equal(sellHalf.curveReserve, 0n);
  assert.equal(buyHalf.tokensOut + buyHalf.burn, buyHalf.tokens);
  assert.equal(sellHalf.tokens + sellHalf.burn, sellHalf.tokensIn);

  const regularTail = TERMINAL_LOT * 3n;
  const nearEnd = { sold: SALE_INVENTORY - regularTail, feeCursor: 0 };
  const leaveSaleable = quoteCost(nearEnd.sold, regularTail - TERMINAL_LOT - 10_000n);
  const bounded = quoteBuy(nearEnd, { maxXlm: leaveSaleable, minTokens: 1n });
  assert.equal(SALE_INVENTORY - bounded.state.sold > GRADUATION_DUST_CAP, true);
  assert.equal(bounded.graduation, null);
  const terminalCost = quoteCost(bounded.state.sold, SALE_INVENTORY - bounded.state.sold);
  assert.throws(() => quoteBuy(bounded.state, {
    maxXlm: terminalCost, minTokens: SALE_INVENTORY - bounded.state.sold
  }), /Minimum token output/);
  assert.equal(bounded.state.graduated, false);
  const finalBuy = quoteBuy(bounded.state, {
    maxXlm: terminalCost, minTokens: 1n
  });
  assert.equal(finalBuy.curveReserve, 0n);
  assert.equal(finalBuy.graduation.dustBurn, 0n);
  assert.equal(finalBuy.graduation.operationTreasuryXlm, 150n * UNIT);
  assert.equal(finalBuy.graduation.xlmReserve, 29_850n * UNIT);
  assert.equal(finalBuy.graduation.tokenReserve, SEED_INVENTORY);
  assert.equal(finalBuy.graduation.lockedGenesisShares, true);
  assert.throws(() => quoteBuy(finalBuy.state, { maxXlm: 1n, minTokens: 1n }), /graduated/);
  const dustFilled = quoteBuy(nearEnd, {
    maxXlm: quoteCost(nearEnd.sold, regularTail - GRADUATION_DUST_CAP), minTokens: 1n
  });
  assert.ok(dustFilled.graduation.dustBurn > 0n);
  assert.ok(dustFilled.graduation.dustBurn <= GRADUATION_DUST_CAP);
  assert.equal(dustFilled.graduation.grossXlmReserve, reserveAt(dustFilled.state.sold));
  assert.equal(dustFilled.graduation.xlmReserve + dustFilled.graduation.operationTreasuryXlm,
    dustFilled.graduation.grossXlmReserve);
  assert.equal(dustFilled.xlmIn, dustFilled.xlmPrincipal +
    Object.values(dustFilled.xlmFees).reduce((sum, amount) => sum + amount, 0n));

  let state = INITIAL_STATE;
  for (let i = 1n; i <= 500n; i += 1n) {
    const priorReserve = reserveAt(state.sold);
    const buy = quoteBuy(state, { maxXlm: i * 10_001n, minTokens: 1n, allowUnspent: true });
    const redemption = quoteSell(buy.state, {
      tokens: buy.tokens, maxTokens: buy.tokens + feeLeg(buy.tokens), minXlm: 1n
    });
    const sumFees = (trade) => Object.values(trade.xlmFees).reduce((sum, amount) => sum + amount, 0n);
    assert.equal(buy.curveReserve - priorReserve, buy.xlmPrincipal);
    assert.equal(buy.xlmIn, buy.xlmPrincipal + sumFees(buy));
    assert.equal(buy.curveReserve - redemption.curveReserve, redemption.xlmPrincipal);
    assert.equal(redemption.xlmOut, redemption.xlmPrincipal - sumFees(redemption));
    assert.equal(redemption.state.sold, state.sold);
    assert.equal(redemption.curveReserve, priorReserve);
    state = redemption.state;
  }
  console.log('Curve reference checks passed, including 500 buy/redemption round trips.');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  checkModel();
}
