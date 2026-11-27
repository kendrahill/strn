#![no_std]

mod events;

use events::{Approve, Burn, CapacityRecycled, Mint, RewardsReleased, Transfer};
use saturn_token_metadata::{self as metadata, MetadataError, MetadataState, TokenMetadata};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env,
    MuxedAddress, String,
};

const DECIMALS: u32 = 7;
const UNIT: i128 = 10_000_000;
const CURVE_INVENTORY: i128 = 400_000_000 * UNIT;
const AMM_INVENTORY: i128 = 300_000_000 * UNIT;
const INITIAL_REWARDS: i128 = 300_000_000 * UNIT;
const CAP: i128 = CURVE_INVENTORY + AMM_INVENTORY + INITIAL_REWARDS;
const REWARD_DAYS: u32 = 3_650;
const DAY_SECONDS: u64 = 86_400;

#[contracttype]
enum Key {
    Roles,
    Supply,
    Balance(Address),
    Allowance(Address, Address),
    BurnedOn(u64),
}

#[contracttype]
#[derive(Clone)]
pub struct LaunchRoles {
    pub curve: Address,
    pub amm_inventory: Address,
    pub reward_controller: Address,
    pub reward_vault: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplyState {
    pub outstanding: i128,
    pub unused_initial: i128,
    pub unused_recycled: i128,
    pub matured_recycled: i128,
    pub eligible_days: u32,
    pub next_reward_day: u64,
}

#[contracttype]
#[derive(Clone)]
struct Allowance {
    amount: i128,
    expiration: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TokenError {
    InvalidAmount = 1,
    InsufficientBalance = 2,
    InsufficientAllowance = 3,
    ExpiredApproval = 4,
    InvalidRewardDay = 5,
    ExcessRecycledRewards = 6,
}

fn day(env: &Env) -> u64 {
    env.ledger().timestamp() / DAY_SECONDS
}

fn check_amount(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }
}

fn write_balance(env: &Env, owner: &Address, balance: i128) {
    let key = Key::Balance(owner.clone());
    let storage = env.storage().persistent();
    if balance == 0 {
        storage.remove(&key);
    } else {
        storage.set(&key, &balance);
        let max_ttl = env.storage().max_ttl();
        storage.extend_ttl(&key, max_ttl / 4, max_ttl / 2);
    }
}

fn balance(env: &Env, owner: &Address) -> i128 {
    env.storage().persistent().get(&Key::Balance(owner.clone())).unwrap_or(0)
}

fn debit(env: &Env, owner: &Address, amount: i128) {
    let previous = balance(env, owner);
    if amount > previous {
        panic_with_error!(env, TokenError::InsufficientBalance);
    }
    if amount > 0 {
        write_balance(env, owner, previous - amount);
    }
}

fn credit(env: &Env, owner: &Address, amount: i128) {
    if amount > 0 {
        write_balance(env, owner, balance(env, owner) + amount);
    }
}

fn mint(env: &Env, to: Address, amount: i128) {
    if amount > 0 {
        credit(env, &to, amount);
        Mint { to, amount }.publish(env);
    }
}

fn move_tokens(env: &Env, from: Address, to: Address, muxed_id: Option<u64>, amount: i128) {
    if from == to {
        if amount > balance(env, &from) {
            panic_with_error!(env, TokenError::InsufficientBalance);
        }
    } else {
        debit(env, &from, amount);
        credit(env, &to, amount);
    }
    Transfer { from, to, amount, to_muxed_id: muxed_id }.publish(env);
}

fn read_allowance(env: &Env, key: &Key) -> Option<Allowance> {
    env.storage().persistent().get::<_, Allowance>(key)
        .filter(|entry| entry.expiration >= env.ledger().sequence())
}

fn consume_allowance(env: &Env, key: Key, amount: i128) {
    if amount == 0 {
        return;
    }
    let mut entry = read_allowance(env, &key)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::InsufficientAllowance));
    if entry.amount < amount {
        panic_with_error!(env, TokenError::InsufficientAllowance);
    }
    entry.amount -= amount;
    if entry.amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &entry);
    }
}

fn burn_tokens(env: &Env, from: Address, amount: i128) {
    if amount == 0 {
        return;
    }
    debit(env, &from, amount);
    let mut state = ProjectToken::supply(env.clone());
    state.outstanding -= amount;
    state.unused_recycled += amount;
    env.storage().instance().set(&Key::Supply, &state);

    let burned_on = day(env);
    let key = Key::BurnedOn(burned_on);
    let storage = env.storage().persistent();
    let previous = storage.get::<_, i128>(&key).unwrap_or(0);
    storage.set(&key, &(previous + amount));
    Burn { from, amount }.publish(env);
    CapacityRecycled { day: burned_on, amount }.publish(env);
}

#[contract]
pub struct ProjectToken;

#[contractimpl]
impl ProjectToken {
    pub fn __constructor(
        env: Env,
        creator: Address,
        fields: TokenMetadata,
        locked: bool,
        roles: LaunchRoles,
    ) {
        if let Err(error) = metadata::validate_project_logo(&fields)
            .and_then(|()| metadata::initialize(&env, creator, fields, locked))
        {
            panic_with_error!(&env, error);
        }
        mint(&env, roles.curve.clone(), CURVE_INVENTORY);
        mint(&env, roles.amm_inventory.clone(), AMM_INVENTORY);
        let instance = env.storage().instance();
        instance.set(&Key::Roles, &roles);
        instance.set(&Key::Supply, &SupplyState {
            outstanding: CURVE_INVENTORY + AMM_INVENTORY,
            unused_initial: INITIAL_REWARDS,
            unused_recycled: 0,
            matured_recycled: 0,
            eligible_days: 0,
            next_reward_day: day(&env),
        });
    }

    pub fn cap() -> i128 { CAP }

    pub fn supply(env: Env) -> SupplyState {
        env.storage().instance().get(&Key::Supply).unwrap()
    }

    pub fn launch_roles(env: Env) -> LaunchRoles {
        env.storage().instance().get(&Key::Roles).unwrap()
    }

    pub fn metadata(env: Env) -> MetadataState { metadata::get(&env) }

    pub fn update_metadata(env: Env, revision: u64, fields: TokenMetadata) -> Result<(), MetadataError> {
        metadata::validate_project_logo(&fields)?;
        metadata::update(&env, revision, fields)
    }

    pub fn lock_metadata(env: Env, revision: u64) -> Result<(), MetadataError> {
        metadata::lock(&env, revision)
    }

    pub fn decimals() -> u32 { DECIMALS }

    pub fn name(env: Env) -> String { metadata::get(&env).metadata.name }

    pub fn symbol(env: Env) -> String { metadata::get(&env).metadata.symbol }

    pub fn balance(env: Env, id: Address) -> i128 { balance(&env, &id) }

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        read_allowance(&env, &Key::Allowance(from, spender)).map_or(0, |entry| entry.amount)
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, live_until_ledger: u32) {
        from.require_auth();
        check_amount(&env, amount);
        if amount > 0 && live_until_ledger < env.ledger().sequence() {
            panic_with_error!(&env, TokenError::ExpiredApproval);
        }
        let key = Key::Allowance(from.clone(), spender.clone());
        let storage = env.storage().persistent();
        if amount == 0 {
            storage.remove(&key);
        } else {
            storage.set(&key, &Allowance { amount, expiration: live_until_ledger });
        }
        Approve { from, spender, amount, live_until_ledger }.publish(&env);
    }

    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        check_amount(&env, amount);
        move_tokens(&env, from, to.address(), to.id(), amount);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        check_amount(&env, amount);
        consume_allowance(&env, Key::Allowance(from.clone(), spender), amount);
        move_tokens(&env, from, to, None, amount);
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        check_amount(&env, amount);
        burn_tokens(&env, from, amount);
    }

    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        check_amount(&env, amount);
        consume_allowance(&env, Key::Allowance(from.clone(), spender), amount);
        burn_tokens(&env, from, amount);
    }

    pub fn burned_on(env: Env, utc_day: u64) -> i128 {
        env.storage().persistent().get(&Key::BurnedOn(utc_day)).unwrap_or(0)
    }

    /// The pinned controller determines daily eligibility and recycled release
    /// smoothing. The token enforces chronology, maturity, budgets and custody.
    pub fn release_day(env: Env, utc_day: u64, eligible: bool, recycled: i128) -> Result<(), TokenError> {
        let roles = Self::launch_roles(env.clone());
        roles.reward_controller.require_auth();
        let mut state = Self::supply(env.clone());
        if utc_day != state.next_reward_day || utc_day >= day(&env) {
            return Err(TokenError::InvalidRewardDay);
        }
        if let Some(prior) = utc_day.checked_sub(1) {
            state.matured_recycled += Self::burned_on(env.clone(), prior);
        }
        if recycled < 0 || recycled > state.matured_recycled || (!eligible && recycled != 0) {
            return Err(TokenError::ExcessRecycledRewards);
        }
        let mut initial = 0;
        if eligible && state.eligible_days < REWARD_DAYS {
            state.eligible_days += 1;
            let cumulative = INITIAL_REWARDS * i128::from(state.eligible_days) / i128::from(REWARD_DAYS);
            initial = cumulative - (INITIAL_REWARDS - state.unused_initial);
        }
        state.unused_initial -= initial;
        state.unused_recycled -= recycled;
        state.matured_recycled -= recycled;
        state.outstanding += initial + recycled;
        state.next_reward_day = utc_day + 1;
        env.storage().instance().set(&Key::Supply, &state);
        mint(&env, roles.reward_vault, initial + recycled);
        RewardsReleased { day: utc_day, eligible, initial, recycled }.publish(&env);
        Ok(())
    }
}

#[cfg(test)]
mod test;
