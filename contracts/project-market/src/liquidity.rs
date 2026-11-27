use super::{check_balances, send, Key, MarketError, ProjectMarket, Reserves, UNIT};
use soroban_sdk::{contractevent, contracttype, token::TokenClient, Address, Env};

const MAX_XLM: i128 = i64::MAX as i128;
const MAX_TOKENS: i128 = 1_000_000_000 * UNIT;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareSupply {
    pub total: i128,
    pub locked: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidityQuote {
    pub xlm: i128,
    pub tokens: i128,
    pub shares: i128,
}

#[contractevent]
struct LiquidityChanged {
    #[topic]
    owner: Address,
    deposit: bool,
    amounts: LiquidityQuote,
}

pub fn lock_genesis(env: &Env, xlm: i128, tokens: i128) {
    // No owner ledger entry is ever created for these shares.
    let locked = ((xlm * tokens) as u128).isqrt() as i128;
    env.storage().instance().set(&Key::ShareSupply, &ShareSupply { total: locked, locked });
}

pub fn supply(env: &Env) -> Option<ShareSupply> {
    env.storage().instance().get(&Key::ShareSupply)
}

pub fn balance(env: &Env, owner: &Address) -> i128 {
    env.storage().persistent().get(&Key::Shares(owner.clone())).unwrap_or(0)
}

fn state(env: &Env) -> Result<(Reserves, ShareSupply), MarketError> {
    let reserves = ProjectMarket::reserves(env.clone()).ok_or(MarketError::Inactive)?;
    let shares = supply(env).ok_or(MarketError::Inactive)?;
    Ok((reserves, shares))
}

fn ceil_ratio(numerator: i128, denominator: i128) -> i128 {
    numerator / denominator + i128::from(numerator % denominator != 0)
}

pub fn quote_deposit(env: &Env, max_xlm: i128, max_tokens: i128) -> Result<LiquidityQuote, MarketError> {
    let (r, s) = state(env)?;
    if max_xlm <= 0 || max_tokens <= 0 {
        return Err(MarketError::InvalidAmount);
    }
    let xlm_budget = max_xlm.min(MAX_XLM - r.xlm);
    let token_budget = max_tokens.min(MAX_TOKENS - r.tokens);
    let shares = (xlm_budget * s.total / r.xlm).min(token_budget * s.total / r.tokens);
    if shares == 0 {
        return Err(MarketError::BelowMinimum);
    }
    Ok(LiquidityQuote {
        xlm: ceil_ratio(shares * r.xlm, s.total),
        tokens: ceil_ratio(shares * r.tokens, s.total),
        shares,
    })
}

pub fn quote_withdraw(env: &Env, shares: i128) -> Result<LiquidityQuote, MarketError> {
    let (r, s) = state(env)?;
    if shares <= 0 || shares > s.total - s.locked {
        return Err(MarketError::InvalidAmount);
    }
    let xlm = shares * r.xlm / s.total;
    let tokens = shares * r.tokens / s.total;
    if xlm == 0 || tokens == 0 {
        return Err(MarketError::BelowMinimum);
    }
    Ok(LiquidityQuote { xlm, tokens, shares })
}

fn record(env: &Env, owner: &Address, reserves: Reserves, supply: ShareSupply, owner_shares: i128) {
    let instance = env.storage().instance();
    instance.set(&Key::Pool, &reserves);
    instance.set(&Key::ShareSupply, &supply);
    let key = Key::Shares(owner.clone());
    let storage = env.storage().persistent();
    if owner_shares == 0 {
        storage.remove(&key);
    } else {
        storage.set(&key, &owner_shares);
    }
}

fn settle(env: &Env, owner: &Address, q: &LiquidityQuote, deposit: bool) -> Result<(), MarketError> {
    let cfg = ProjectMarket::config(env.clone());
    let here = env.current_contract_address();
    if owner == &here {
        return Err(MarketError::InvalidAmount);
    }
    let (from, to) = if deposit { (owner, &here) } else { (&here, owner) };
    for (asset, amount) in [(&cfg.xlm, q.xlm), (&cfg.addresses.token, q.tokens)] {
        let token = TokenClient::new(env, asset);
        let before = token.balance(to);
        send(&token, from, to, amount);
        if token.balance(to) - before != amount {
            return Err(MarketError::Insolvent);
        }
    }
    Ok(())
}

pub fn deposit(
    env: &Env, owner: &Address, max_xlm: i128, max_tokens: i128, min_shares: i128, deadline: u64,
) -> Result<LiquidityQuote, MarketError> {
    if env.ledger().timestamp() > deadline {
        return Err(MarketError::Expired);
    }
    let q = quote_deposit(env, max_xlm, max_tokens)?;
    if min_shares <= 0 || q.shares < min_shares {
        return Err(MarketError::Slippage);
    }
    let (mut r, mut s) = state(env)?;
    let cfg = ProjectMarket::config(env.clone());
    check_balances(env, &cfg, r.xlm, r.tokens)?;
    settle(env, owner, &q, true)?;
    r.xlm += q.xlm;
    r.tokens += q.tokens;
    s.total += q.shares;
    check_balances(env, &cfg, r.xlm, r.tokens)?;
    record(env, owner, r, s, balance(env, owner) + q.shares);
    LiquidityChanged { owner: owner.clone(), deposit: true, amounts: q.clone() }.publish(env);
    Ok(q)
}

pub fn withdraw(
    env: &Env, owner: &Address, shares: i128, min_xlm: i128, min_tokens: i128, deadline: u64,
) -> Result<LiquidityQuote, MarketError> {
    if env.ledger().timestamp() > deadline {
        return Err(MarketError::Expired);
    }
    let owned = balance(env, owner);
    if owned < shares {
        return Err(MarketError::InsufficientShares);
    }
    let q = quote_withdraw(env, shares)?;
    if min_xlm <= 0 || min_tokens <= 0 || q.xlm < min_xlm || q.tokens < min_tokens {
        return Err(MarketError::Slippage);
    }
    let (mut r, mut s) = state(env)?;
    let cfg = ProjectMarket::config(env.clone());
    check_balances(env, &cfg, r.xlm, r.tokens)?;
    settle(env, owner, &q, false)?;
    r.xlm -= q.xlm;
    r.tokens -= q.tokens;
    s.total -= shares;
    check_balances(env, &cfg, r.xlm, r.tokens)?;
    record(env, owner, r, s, owned - shares);
    LiquidityChanged { owner: owner.clone(), deposit: false, amounts: q.clone() }.publish(env);
    Ok(q)
}
