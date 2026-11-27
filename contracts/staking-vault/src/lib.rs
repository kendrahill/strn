#![no_std]

mod position;
mod rewards;
pub use position::Position;
pub use rewards::{ClaimReceipt, RewardEpoch, RewardState};
use position::LOYALTY_DAYS;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token::TokenClient,
    Address, Env,
};

const DAY_SECONDS: u64 = 86_400;
const MAX_PRINCIPAL: i128 = 1_000_000_000 * 10_000_000;
const MAX_ROLL_DAYS: u32 = 16;

#[contracttype]
enum Key {
    Token,
    Principal,
    Pool,
    Owner(Address),
    Delta(u64),
    DayPower(u64),
    Checkpoint(Address, u64),
    RewardState,
    RewardEpoch(u64),
    Claim(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pool {
    pub day: u64,
    pub scaled_power: i128,
    pub daily_increment: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StakeCheckpoint {
    pub principal: i128,
    pub age_credit: i128,
}

#[contracttype]
struct Delta {
    power: i128,
    increment: i128,
}

#[contractevent]
pub struct StakeIntent {
    #[topic]
    pub owner: Address,
    pub effective_day: u64,
    pub deposit: i128,
    pub exit: i128,
}

#[contractevent]
pub struct PrincipalPaid {
    #[topic]
    pub owner: Address,
    pub amount: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    InvalidAmount = 1,
    InsufficientPrincipal = 2,
    NoPrincipalToCollect = 3,
    InvalidBatch = 5,
    InexactTransfer = 6,
    CapacityExceeded = 7,
    EpochNotReady = 8,
    RewardInsolvent = 9,
    InvalidRewardRelease = 10,
}

fn today(env: &Env) -> u64 { env.ledger().timestamp() / DAY_SECONDS }

fn position(env: &Env, owner: &Address) -> Position {
    let day = today(env);
    let mut position: Position = env.storage().persistent().get(&Key::Owner(owner.clone()))
        .unwrap_or_else(|| Position::empty(day));
    position.advance(day);
    position
}

fn save_position(env: &Env, owner: &Address, position: &Position) {
    env.storage().persistent().set(&Key::Owner(owner.clone()), position);
}

fn edit_delta(env: &Env, day: u64, power: i128, increment: i128) {
    if power == 0 && increment == 0 {
        return;
    }
    let key = Key::Delta(day);
    let storage = env.storage().persistent();
    let mut delta: Delta = storage.get(&key).unwrap_or(Delta { power: 0, increment: 0 });
    delta.power += power;
    delta.increment += increment;
    if delta.power == 0 && delta.increment == 0 {
        storage.remove(&key);
    } else {
        storage.set(&key, &delta);
    }
}

// Add or remove a trajectory from the effective day forward. The increment
// changes at the start and end of the linear ramp and on its final partial day.
fn schedule(env: &Env, position: &Position, sign: i128) {
    edit_delta(env, position.day, sign * position.power(), 0);
    if position.principal == 0 {
        return;
    }
    let remaining = position.principal * LOYALTY_DAYS - position.age_credit;
    let full_days = (remaining / position.principal) as u64;
    let partial = remaining % position.principal;
    if full_days > 0 {
        let increment = sign * position.principal;
        edit_delta(env, position.day + 1, 0, increment);
        edit_delta(env, position.day + full_days + 1, 0, -increment);
    }
    if partial > 0 {
        edit_delta(env, position.day + full_days + 1, 0, sign * partial);
        edit_delta(env, position.day + full_days + 2, 0, -sign * partial);
    }
}

fn update_intent(env: &Env, owner: &Address, old: Position, new: &Position) {
    let effective_day = new.day + 1;
    let mut before = old;
    let mut after = new.clone();
    before.advance(effective_day);
    after.advance(effective_day);
    schedule(env, &before, -1);
    schedule(env, &after, 1);
    save_position(env, owner, new);
    env.storage().persistent().set(
        &Key::Checkpoint(owner.clone(), effective_day),
        &StakeCheckpoint { principal: after.principal, age_credit: after.age_credit },
    );
    rewards::record_first_intent(env, owner, effective_day);
    StakeIntent {
        owner: owner.clone(), effective_day, deposit: new.pending_deposit, exit: new.pending_exit,
    }.publish(env);
}

fn roll(env: &Env, max_days: u32) -> Pool {
    let mut pool = StakingVault::pool(env.clone());
    let target = today(env).min(pool.day + u64::from(max_days));
    let storage = env.storage().persistent();
    while pool.day < target {
        pool.day += 1;
        let key = Key::Delta(pool.day);
        let delta: Delta = storage.get(&key).unwrap_or(Delta { power: 0, increment: 0 });
        pool.daily_increment += delta.increment;
        pool.scaled_power += pool.daily_increment + delta.power;
        storage.remove(&key);
        storage.set(&Key::DayPower(pool.day), &pool.scaled_power);
    }
    env.storage().instance().set(&Key::Pool, &pool);
    pool
}

fn send(env: &Env, receiver: &Address, amount: i128) -> Result<(), VaultError> {
    let asset = StakingVault::asset(env.clone());
    let token = TokenClient::new(env, &asset);
    let before = token.balance(receiver);
    token.transfer(&env.current_contract_address(), receiver, &amount);
    if token.balance(receiver) - before != amount {
        return Err(VaultError::InexactTransfer);
    }
    if token.balance(&env.current_contract_address()) <
        StakingVault::principal_liability(env.clone()) + rewards::state(env).escrowed
    {
        return Err(VaultError::RewardInsolvent);
    }
    Ok(())
}

fn update_liability(env: &Env, delta: i128) {
    let total = StakingVault::principal_liability(env.clone()) + delta;
    env.storage().instance().set(&Key::Principal, &total);
}

#[contract]
pub struct StakingVault;

#[contractimpl]
impl StakingVault {
    pub fn __constructor(env: Env, token: Address) {
        let day = today(&env);
        let instance = env.storage().instance();
        instance.set(&Key::Token, &token);
        instance.set(&Key::Principal, &0i128);
        instance.set(&Key::Pool, &Pool { day, scaled_power: 0, daily_increment: 0 });
        env.storage().persistent().set(&Key::DayPower(day), &0i128);
        rewards::initialize(&env, day);
    }

    pub fn asset(env: Env) -> Address { env.storage().instance().get(&Key::Token).unwrap() }

    pub fn principal_liability(env: Env) -> i128 {
        env.storage().instance().get(&Key::Principal).unwrap()
    }

    pub fn position(env: Env, owner: Address) -> Position { position(&env, &owner) }

    pub fn pool(env: Env) -> Pool { env.storage().instance().get(&Key::Pool).unwrap() }

    pub fn day_power(env: Env, day: u64) -> Option<i128> {
        env.storage().persistent().get(&Key::DayPower(day))
    }

    pub fn checkpoint(env: Env, owner: Address, day: u64) -> Option<StakeCheckpoint> {
        env.storage().persistent().get(&Key::Checkpoint(owner, day))
    }

    pub fn roll(env: Env, max_days: u32) -> Result<Pool, VaultError> {
        if !(1..=MAX_ROLL_DAYS).contains(&max_days) {
            return Err(VaultError::InvalidBatch);
        }
        Ok(roll(&env, max_days))
    }

    pub fn reward_state(env: Env) -> RewardState { rewards::state(&env) }

    pub fn reward_epoch(env: Env, day: u64) -> Option<RewardEpoch> {
        env.storage().persistent().get(&Key::RewardEpoch(day))
    }

    pub fn settle_rewards(env: Env) -> Result<RewardEpoch, VaultError> {
        rewards::settle(&env)
    }

    pub fn claim(env: Env, owner: Address, max_days: u32) -> Result<ClaimReceipt, VaultError> {
        owner.require_auth();
        rewards::claim(&env, &owner, max_days)
    }

    pub fn queue_stake(env: Env, owner: Address, amount: i128) -> Result<(), VaultError> {
        owner.require_auth();
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        if amount > MAX_PRINCIPAL - Self::principal_liability(env.clone()) {
            return Err(VaultError::CapacityExceeded);
        }
        let old = position(&env, &owner);
        let mut new = old.clone();
        new.pending_deposit += amount;
        let asset = Self::asset(env.clone());
        let token = TokenClient::new(&env, &asset);
        let vault = env.current_contract_address();
        let before = token.balance(&vault);
        token.transfer(&owner, &vault, &amount);
        if token.balance(&vault) - before != amount {
            return Err(VaultError::InexactTransfer);
        }
        update_liability(&env, amount);
        update_intent(&env, &owner, old, &new);
        Ok(())
    }

    pub fn cancel_deposit(env: Env, owner: Address, amount: i128) -> Result<(), VaultError> {
        owner.require_auth();
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        let old = position(&env, &owner);
        if amount > old.pending_deposit {
            return Err(VaultError::InsufficientPrincipal);
        }
        let mut new = old.clone();
        new.pending_deposit -= amount;
        update_liability(&env, -amount);
        update_intent(&env, &owner, old, &new);
        send(&env, &owner, amount)
    }

    pub fn queue_exit(env: Env, owner: Address, amount: i128) -> Result<(), VaultError> {
        owner.require_auth();
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        let old = position(&env, &owner);
        if amount > old.principal - old.pending_exit {
            return Err(VaultError::InsufficientPrincipal);
        }
        let mut new = old.clone();
        new.pending_exit += amount;
        update_intent(&env, &owner, old, &new);
        Ok(())
    }

    pub fn collect_exit(env: Env, owner: Address) -> Result<i128, VaultError> {
        owner.require_auth();
        let mut position = position(&env, &owner);
        let amount = position.collectible;
        if amount == 0 {
            return Err(VaultError::NoPrincipalToCollect);
        }
        position.collectible = 0;
        save_position(&env, &owner, &position);
        update_liability(&env, -amount);
        send(&env, &owner, amount)?;
        Ok(amount)
    }
}

#[cfg(test)]
mod test;
