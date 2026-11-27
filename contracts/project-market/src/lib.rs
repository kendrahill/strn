#![no_std]

mod liquidity;
pub use liquidity::{LiquidityQuote, ShareSupply};

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short,
    token::TokenClient, Address, Env, IntoVal, U256,
};

const UNIT: i128 = 10_000_000;
const SALE: i128 = 400_000_000 * UNIT;
const SEED: i128 = 300_000_000 * UNIT;
const FEE_DIVISOR: i128 = 200;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchAddresses {
    pub token: Address,
    pub vault: Address,
    pub curve: Address,
    pub pool: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketConfig {
    pub creator: Address,
    pub addresses: LaunchAddresses,
    pub xlm: Address,
    pub treasury: Address,
    pub strn_rewards: Address,
    pub foundation: Address,
    pub target: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurveState {
    pub sold: i128,
    pub graduated: bool,
    pub fee_cursor: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Graduation {
    pub dust: i128,
    pub gross_xlm: i128,
    pub fee: i128,
    pub pool_xlm: i128,
    pub pool_tokens: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trade {
    pub tokens: i128,
    pub xlm: i128,
    pub token_fee: i128,
    pub xlm_fee: i128,
    pub input: i128,
    pub output: i128,
    pub graduation: Option<Graduation>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reserves {
    pub xlm: i128,
    pub tokens: i128,
}

#[contracttype]
enum Key {
    Config,
    Curve,
    Pool,
    PoolCursor,
    ShareSupply,
    Shares(Address),
}

#[contractevent]
pub struct TradeSettled {
    #[topic]
    pub trader: Address,
    pub buy: bool,
    pub trade: Trade,
}

#[contractevent]
pub struct Graduated {
    #[topic]
    pub pool: Address,
    pub amounts: Graduation,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MarketError {
    InvalidConfig = 1,
    WrongVenue = 2,
    Inactive = 3,
    InvalidAmount = 4,
    BelowMinimum = 5,
    Slippage = 6,
    Expired = 7,
    Insolvent = 8,
    AlreadySeeded = 9,
    InsufficientShares = 10,
}

fn wide(env: &Env, value: i128) -> U256 {
    U256::from_u128(env, value as u128)
}

fn reserve(env: &Env, target: i128, sold: i128) -> i128 {
    wide(env, target).mul(&wide(env, sold)).mul(&wide(env, 2 * SALE + sold))
        .div(&wide(env, 3 * SALE * SALE)).to_u128().unwrap() as i128
}

fn trade(buy: bool, tokens: i128, xlm: i128) -> Result<Trade, MarketError> {
    let token_fee = tokens / FEE_DIVISOR;
    let xlm_fee = xlm / FEE_DIVISOR;
    if token_fee == 0 || xlm_fee == 0 {
        return Err(MarketError::BelowMinimum);
    }
    let (input, output) = if buy {
        (xlm + xlm_fee, tokens - token_fee)
    } else {
        (tokens + token_fee, xlm - xlm_fee)
    };
    Ok(Trade { tokens, xlm, token_fee, xlm_fee, input, output, graduation: None })
}

fn curve_quote(
    env: &Env, config: &MarketConfig, state: &CurveState, buy: bool, tokens: i128,
) -> Result<Trade, MarketError> {
    if state.graduated {
        return Err(MarketError::Inactive);
    }
    if tokens <= 0 || tokens > if buy { SALE - state.sold } else { state.sold } {
        return Err(MarketError::InvalidAmount);
    }
    let after = if buy { state.sold + tokens } else { state.sold - tokens };
    let before = reserve(env, config.target, state.sold);
    let funded = reserve(env, config.target, after);
    let mut quote = trade(buy, tokens, if buy { funded - before } else { before - funded })?;
    if buy && ((SALE - after) / FEE_DIVISOR == 0 || (config.target - funded) / FEE_DIVISOR == 0) {
        let fee = funded / FEE_DIVISOR;
        quote.graduation = Some(Graduation {
            dust: SALE - after, gross_xlm: funded, fee, pool_xlm: funded - fee, pool_tokens: SEED,
        });
    }
    Ok(quote)
}

fn send(token: &TokenClient, from: &Address, to: &Address, amount: i128) {
    if amount > 0 {
        token.transfer(from, to, &amount);
    }
}

fn split_xlm(
    token: &TokenClient, from: &Address, config: &MarketConfig, fee: i128, cursor: &mut u32,
) {
    let quarter = fee / 4;
    let mut parts = [quarter, quarter, 2 * quarter];
    let rotation = [2, 0, 2, 1];
    for _ in 0..fee % 4 {
        parts[rotation[*cursor as usize]] += 1;
        *cursor = (*cursor + 1) % 4;
    }
    send(token, from, &config.creator, parts[0]);
    send(token, from, &config.foundation, parts[1]);
    send(token, from, &config.strn_rewards, parts[2]);
}

fn check_balances(
    env: &Env, config: &MarketConfig, xlm: i128, tokens: i128,
) -> Result<(), MarketError> {
    let address = env.current_contract_address();
    if TokenClient::new(env, &config.xlm).balance(&address) < xlm
        || TokenClient::new(env, &config.addresses.token).balance(&address) < tokens
    {
        return Err(MarketError::Insolvent);
    }
    Ok(())
}

fn limits(env: &Env, quote: &Trade, max_input: i128, min_output: i128, deadline: u64)
    -> Result<(), MarketError>
{
    if env.ledger().timestamp() > deadline {
        return Err(MarketError::Expired);
    }
    if min_output <= 0 || quote.input > max_input || quote.output < min_output {
        return Err(MarketError::Slippage);
    }
    Ok(())
}

fn pool_quote(reserves: &Reserves, buy: bool, principal: i128) -> Result<Trade, MarketError> {
    // These bounds also keep constant-product arithmetic within i128.
    if principal <= 0 || principal > i128::from(i64::MAX) {
        return Err(MarketError::InvalidAmount);
    }
    if buy {
        trade(true, reserves.tokens * principal / (reserves.xlm + principal), principal)
    } else {
        trade(false, principal, reserves.xlm * principal / (reserves.tokens + principal))
    }
}

#[contract]
pub struct ProjectMarket;

#[contractimpl]
impl ProjectMarket {
    pub fn __constructor(env: Env, config: MarketConfig) -> Result<(), MarketError> {
        let here = env.current_contract_address();
        if config.addresses.curve == config.addresses.pool || config.xlm == config.addresses.token
            || !(UNIT..=1_000_000 * UNIT).contains(&config.target)
            || (here != config.addresses.curve && here != config.addresses.pool)
        {
            return Err(MarketError::InvalidConfig);
        }
        let storage = env.storage().instance();
        if here == config.addresses.curve {
            check_balances(&env, &config, 0, SALE + SEED)?;
            storage.set(&Key::Curve, &CurveState { sold: 0, graduated: false, fee_cursor: 0 });
        }
        storage.set(&Key::Config, &config);
        Ok(())
    }

    pub fn config(env: Env) -> MarketConfig {
        env.storage().instance().get(&Key::Config).unwrap()
    }

    pub fn curve_state(env: Env) -> Result<CurveState, MarketError> {
        env.storage().instance().get(&Key::Curve).ok_or(MarketError::WrongVenue)
    }

    pub fn reserves(env: Env) -> Option<Reserves> {
        env.storage().instance().get(&Key::Pool)
    }

    pub fn quote_curve(env: Env, buy: bool, tokens: i128) -> Result<Trade, MarketError> {
        let config = Self::config(env.clone());
        let state = Self::curve_state(env.clone())?;
        curve_quote(&env, &config, &state, buy, tokens)
    }

    pub fn trade_curve(
        env: Env, trader: Address, buy: bool, tokens: i128,
        max_input: i128, min_output: i128, deadline: u64,
    ) -> Result<Trade, MarketError> {
        trader.require_auth();
        let config = Self::config(env.clone());
        let mut state = Self::curve_state(env.clone())?;
        let quote = curve_quote(&env, &config, &state, buy, tokens)?;
        limits(&env, &quote, max_input, min_output, deadline)?;
        check_balances(&env, &config, reserve(&env, config.target, state.sold), SALE - state.sold + SEED)?;
        let here = env.current_contract_address();
        let asset = TokenClient::new(&env, &config.addresses.token);
        let xlm = TokenClient::new(&env, &config.xlm);
        if buy {
            send(&xlm, &trader, &here, quote.input);
            send(&asset, &here, &trader, quote.output);
            state.sold += tokens;
        } else {
            send(&asset, &trader, &here, quote.input);
            send(&xlm, &here, &trader, quote.output);
            state.sold -= tokens;
        }
        asset.burn(&here, &quote.token_fee);
        split_xlm(&xlm, &here, &config, quote.xlm_fee, &mut state.fee_cursor);
        if let Some(amounts) = &quote.graduation {
            if amounts.dust > 0 {
                asset.burn(&here, &amounts.dust);
            }
            send(&xlm, &here, &config.treasury, amounts.fee);
            send(&xlm, &here, &config.addresses.pool, amounts.pool_xlm);
            send(&asset, &here, &config.addresses.pool, SEED);
            env.invoke_contract::<()>(
                &config.addresses.pool, &symbol_short!("seed"),
                (amounts.pool_xlm, SEED).into_val(&env),
            );
            state.graduated = true;
            Graduated { pool: config.addresses.pool, amounts: amounts.clone() }.publish(&env);
        } else {
            check_balances(&env, &config, reserve(&env, config.target, state.sold), SALE - state.sold + SEED)?;
        }
        env.storage().instance().set(&Key::Curve, &state);
        TradeSettled { trader, buy, trade: quote.clone() }.publish(&env);
        Ok(quote)
    }

    pub fn seed(env: Env, xlm: i128, tokens: i128) -> Result<(), MarketError> {
        let config = Self::config(env.clone());
        if env.current_contract_address() != config.addresses.pool {
            return Err(MarketError::WrongVenue);
        }
        config.addresses.curve.require_auth();
        let storage = env.storage().instance();
        if storage.has(&Key::Pool) {
            return Err(MarketError::AlreadySeeded);
        }
        if xlm <= 0 || xlm > config.target || tokens != SEED {
            return Err(MarketError::InvalidAmount);
        }
        check_balances(&env, &config, xlm, tokens)?;
        storage.set(&Key::Pool, &Reserves { xlm, tokens });
        storage.set(&Key::PoolCursor, &0u32);
        liquidity::lock_genesis(&env, xlm, tokens);
        Ok(())
    }

    pub fn share_supply(env: Env) -> Option<ShareSupply> {
        liquidity::supply(&env)
    }

    pub fn lp_balance(env: Env, owner: Address) -> i128 {
        liquidity::balance(&env, &owner)
    }

    pub fn quote_deposit(env: Env, max_xlm: i128, max_tokens: i128) -> Result<LiquidityQuote, MarketError> {
        liquidity::quote_deposit(&env, max_xlm, max_tokens)
    }

    pub fn deposit(
        env: Env, owner: Address, max_xlm: i128, max_tokens: i128,
        min_shares: i128, deadline: u64,
    ) -> Result<LiquidityQuote, MarketError> {
        owner.require_auth();
        liquidity::deposit(&env, &owner, max_xlm, max_tokens, min_shares, deadline)
    }

    pub fn quote_withdraw(env: Env, shares: i128) -> Result<LiquidityQuote, MarketError> {
        liquidity::quote_withdraw(&env, shares)
    }

    pub fn withdraw(
        env: Env, owner: Address, shares: i128, min_xlm: i128, min_tokens: i128,
        deadline: u64,
    ) -> Result<LiquidityQuote, MarketError> {
        owner.require_auth();
        liquidity::withdraw(&env, &owner, shares, min_xlm, min_tokens, deadline)
    }

    pub fn quote_swap(env: Env, buy: bool, principal: i128) -> Result<Trade, MarketError> {
        let reserves = Self::reserves(env).ok_or(MarketError::Inactive)?;
        pool_quote(&reserves, buy, principal)
    }

    pub fn swap(
        env: Env, trader: Address, buy: bool, principal: i128,
        max_input: i128, min_output: i128, deadline: u64,
    ) -> Result<Trade, MarketError> {
        trader.require_auth();
        let config = Self::config(env.clone());
        let before = Self::reserves(env.clone()).ok_or(MarketError::Inactive)?;
        let quote = pool_quote(&before, buy, principal)?;
        limits(&env, &quote, max_input, min_output, deadline)?;
        let retained_xlm = quote.xlm_fee / 2;
        let retained_tokens = quote.token_fee / 2;
        let after = if buy {
            Reserves {
                xlm: before.xlm + quote.xlm + retained_xlm,
                tokens: before.tokens - quote.tokens + retained_tokens,
            }
        } else {
            Reserves {
                xlm: before.xlm - quote.xlm + retained_xlm,
                tokens: before.tokens + quote.tokens + retained_tokens,
            }
        };
        if after.xlm > i128::from(i64::MAX) || after.tokens > 1_000_000_000 * UNIT {
            return Err(MarketError::InvalidAmount);
        }
        check_balances(&env, &config, before.xlm, before.tokens)?;
        let here = env.current_contract_address();
        let asset = TokenClient::new(&env, &config.addresses.token);
        let xlm = TokenClient::new(&env, &config.xlm);
        if buy {
            send(&xlm, &trader, &here, quote.input);
            send(&asset, &here, &trader, quote.output);
        } else {
            send(&asset, &trader, &here, quote.input);
            send(&xlm, &here, &trader, quote.output);
        }
        asset.burn(&here, &(quote.token_fee - retained_tokens));
        let storage = env.storage().instance();
        let mut cursor = storage.get(&Key::PoolCursor).unwrap();
        split_xlm(&xlm, &here, &config, quote.xlm_fee - retained_xlm, &mut cursor);
        check_balances(&env, &config, after.xlm, after.tokens)?;
        storage.set(&Key::PoolCursor, &cursor);
        storage.set(&Key::Pool, &after);
        TradeSettled { trader, buy, trade: quote.clone() }.publish(&env);
        Ok(quote)
    }
}
