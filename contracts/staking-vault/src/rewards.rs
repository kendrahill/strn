use super::{send, today, Key, StakingVault, VaultError, LOYALTY_DAYS, MAX_PRINCIPAL};
use soroban_sdk::{
    contractclient, contractevent, contracttype, token::TokenClient, Address, Env, Vec,
};

const RECYCLE_DAYS: u32 = 30;
const CLAIM_DAYS: u32 = 16;
const PRECISION: i128 = 1_000_000_000_000_000_000;

#[contractclient(name = "ProjectRewardsClient")]
#[allow(dead_code)]
pub trait ProjectRewards {
    fn burned_on(env: Env, utc_day: u64) -> i128;
    fn release_day(env: Env, utc_day: u64, eligible: bool, recycled: i128);
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardState {
    pub next_day: u64,
    pub escrowed: i128,
    // The head slot is the next eligible day. Empty days leave it in place.
    pub recycle_head: u32,
    pub recycle_slots: Vec<i128>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardEpoch {
    pub day: u64,
    pub power: i128,
    pub minted: i128,
    pub recycled: i128,
}

#[contracttype]
struct ClaimState {
    next_day: u64,
    principal: i128,
    base_credit: i128,
    checkpoint_day: u64,
    fraction: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimReceipt {
    pub paid: i128,
    pub next_day: u64,
    pub has_more: bool,
}

#[contractevent]
pub struct RewardsSettled {
    #[topic]
    pub day: u64,
    pub power: i128,
    pub minted: i128,
    pub recycled: i128,
}

#[contractevent]
pub struct RewardsClaimed {
    #[topic]
    pub owner: Address,
    pub amount: i128,
    pub next_day: u64,
}

pub fn initialize(env: &Env, day: u64) {
    let mut recycle_slots = Vec::new(env);
    for _ in 0..RECYCLE_DAYS {
        recycle_slots.push_back(0);
    }
    env.storage().instance().set(&Key::RewardState, &RewardState {
        next_day: day, escrowed: 0, recycle_head: 0, recycle_slots,
    });
}

pub fn state(env: &Env) -> RewardState {
    env.storage().instance().get(&Key::RewardState).unwrap()
}

fn new_claim(day: u64) -> ClaimState {
    ClaimState { next_day: day, principal: 0, base_credit: 0, checkpoint_day: day, fraction: 0 }
}

pub fn record_first_intent(env: &Env, owner: &Address, effective_day: u64) {
    let key = Key::Claim(owner.clone());
    let storage = env.storage().persistent();
    if !storage.has(&key) {
        storage.set(&key, &new_claim(effective_day));
    }
}

fn queue_cohort(rewards: &mut RewardState, burned: i128) -> Result<(), VaultError> {
    if !(0..=MAX_PRINCIPAL).contains(&burned) {
        return Err(VaultError::InvalidRewardRelease);
    }
    if burned == 0 {
        return Ok(());
    }
    let duration = i128::from(RECYCLE_DAYS);
    for offset in 0..RECYCLE_DAYS {
        let elapsed = i128::from(offset);
        let portion = burned * (elapsed + 1) / duration - burned * elapsed / duration;
        let slot = (rewards.recycle_head + offset) % RECYCLE_DAYS;
        let pending = rewards.recycle_slots.get_unchecked(slot);
        rewards.recycle_slots.set(slot, pending + portion);
    }
    Ok(())
}

fn next_recycled(rewards: &mut RewardState, eligible: bool) -> i128 {
    if !eligible {
        return 0;
    }
    let slot = rewards.recycle_head;
    let recycled = rewards.recycle_slots.get_unchecked(slot);
    rewards.recycle_slots.set(slot, 0);
    rewards.recycle_head = (slot + 1) % RECYCLE_DAYS;
    recycled
}

pub fn settle(env: &Env) -> Result<RewardEpoch, VaultError> {
    let mut rewards = state(env);
    let day = rewards.next_day;
    if day >= today(env) {
        return Err(VaultError::EpochNotReady);
    }
    let power = StakingVault::day_power(env.clone(), day).ok_or(VaultError::EpochNotReady)?;
    let asset = StakingVault::asset(env.clone());
    let project = ProjectRewardsClient::new(env, &asset);
    if let Some(prior) = day.checked_sub(1) {
        queue_cohort(&mut rewards, project.burned_on(&prior))?;
    }
    let token = TokenClient::new(env, &asset);
    let vault = env.current_contract_address();
    let before = token.balance(&vault);
    if before < StakingVault::principal_liability(env.clone()) + rewards.escrowed {
        return Err(VaultError::RewardInsolvent);
    }
    let eligible = power > 0;
    let recycled = next_recycled(&mut rewards, eligible);
    project.release_day(&day, &eligible, &recycled);
    let minted = token.balance(&vault) - before;
    if minted < recycled || minted > MAX_PRINCIPAL || (!eligible && minted != 0) {
        return Err(VaultError::InvalidRewardRelease);
    }
    rewards.next_day += 1;
    rewards.escrowed += minted;
    let epoch = RewardEpoch { day, power, minted, recycled };
    env.storage().instance().set(&Key::RewardState, &rewards);
    env.storage().persistent().set(&Key::RewardEpoch(day), &epoch);
    RewardsSettled { day, power, minted, recycled }.publish(env);
    Ok(epoch)
}

fn earned_scaled(epoch: &RewardEpoch, owner_power: i128) -> Result<i128, VaultError> {
    if owner_power < 0 || owner_power > epoch.power {
        return Err(VaultError::InvalidRewardRelease);
    }
    if owner_power == 0 || epoch.minted == 0 {
        return Ok(0);
    }
    let numerator = epoch.minted * owner_power;
    let whole = numerator / epoch.power;
    let remainder = numerator % epoch.power;
    Ok(whole * PRECISION + remainder * PRECISION / epoch.power)
}

pub fn claim(env: &Env, owner: &Address, max_days: u32) -> Result<ClaimReceipt, VaultError> {
    if !(1..=CLAIM_DAYS).contains(&max_days) {
        return Err(VaultError::InvalidBatch);
    }
    let mut rewards = state(env);
    let key = Key::Claim(owner.clone());
    let storage = env.storage().persistent();
    let Some(mut cursor) = storage.get::<_, ClaimState>(&key) else {
        return Ok(ClaimReceipt { paid: 0, next_day: rewards.next_day, has_more: false });
    };
    let target = rewards.next_day.min(cursor.next_day + u64::from(max_days));
    let mut accrued = cursor.fraction;
    while cursor.next_day < target {
        let day = cursor.next_day;
        if let Some(checkpoint) = StakingVault::checkpoint(env.clone(), owner.clone(), day) {
            cursor.principal = checkpoint.principal;
            cursor.base_credit = checkpoint.age_credit;
            cursor.checkpoint_day = day;
        }
        let elapsed = i128::from(day - cursor.checkpoint_day).min(LOYALTY_DAYS);
        let max_credit = cursor.principal * LOYALTY_DAYS;
        let credit = (cursor.base_credit + cursor.principal * elapsed).min(max_credit);
        let epoch = StakingVault::reward_epoch(env.clone(), day).ok_or(VaultError::EpochNotReady)?;
        accrued += earned_scaled(&epoch, max_credit + credit)?;
        cursor.next_day += 1;
    }
    let paid = accrued / PRECISION;
    if paid > rewards.escrowed {
        return Err(VaultError::RewardInsolvent);
    }
    cursor.fraction = accrued % PRECISION;
    storage.set(&key, &cursor);
    if paid > 0 {
        rewards.escrowed -= paid;
        env.storage().instance().set(&Key::RewardState, &rewards);
        send(env, owner, paid)?;
        RewardsClaimed { owner: owner.clone(), amount: paid, next_day: cursor.next_day }.publish(env);
    }
    Ok(ClaimReceipt { paid, next_day: cursor.next_day, has_more: cursor.next_day < rewards.next_day })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn burn_cohorts_release_exactly_and_pause_on_empty_days() {
        let env = Env::default();
        let mut recycle_slots = Vec::new(&env);
        for _ in 0..RECYCLE_DAYS {
            recycle_slots.push_back(0);
        }
        let mut state = RewardState { next_day: 0, escrowed: 0, recycle_head: 29, recycle_slots };
        queue_cohort(&mut state, 61).unwrap();
        assert_eq!(next_recycled(&mut state, false), 0);
        assert_eq!(state.recycle_head, 29);
        assert_eq!(next_recycled(&mut state, true), 2);
        queue_cohort(&mut state, 17).unwrap();
        let mut released = 2;
        for i in 2..=31 {
            let expected = 61 * i.min(30) / 30 - 61 * (i - 1).min(30) / 30
                + 17 * (i - 1) / 30 - 17 * (i - 2) / 30;
            let current = next_recycled(&mut state, true);
            assert_eq!(current, expected);
            released += current;
        }
        assert_eq!(released, 78);
        assert_eq!(next_recycled(&mut state, true), 0);
    }
}
