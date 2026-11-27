#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env,
};

const XLM: i128 = 10_000_000;
const INITIAL_CREATION_FEE: i128 = 100 * XLM;
const MAX_CREATION_FEE: i128 = 1_000 * XLM;
const INITIAL_GRADUATION_TARGET: i128 = 30_000 * XLM;
const MAX_GRADUATION_TARGET: i128 = 1_000_000 * XLM;
const DELAY_SECONDS: u64 = 48 * 60 * 60;
const GRADUATION_FEE_DIVISOR: i128 = 200; // 50 bps

#[contracttype]
enum Key {
    Owner,
    Treasury,
    CreationFee,
    Pending,
    GraduationTarget,
    PendingTarget,
    Nonce,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreationFeeChange {
    pub nonce: u64,
    pub fee: i128,
    pub executable_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeePolicy {
    pub treasury: Address,
    pub creation_fee: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraduationTargetChange {
    pub nonce: u64,
    pub target: i128,
    pub executable_at: u64,
}

#[contractevent]
pub struct TargetProposed {
    pub nonce: u64,
    pub target: i128,
    pub executable_at: u64,
}

#[contractevent]
pub struct TargetChanged {
    pub nonce: u64,
    pub previous: i128,
    pub current: i128,
}

#[contractevent]
pub struct TargetCanceled {
    pub nonce: u64,
    pub target: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraduationAmounts {
    pub fee: i128,
    pub pool_xlm: i128,
}

#[contractevent]
pub struct FeeProposed {
    pub nonce: u64,
    pub fee: i128,
    pub executable_at: u64,
}

#[contractevent]
pub struct FeeChanged {
    pub nonce: u64,
    pub previous: i128,
    pub current: i128,
}

#[contractevent]
pub struct FeeCanceled {
    pub nonce: u64,
    pub fee: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeeError {
    InvalidFee = 1,
    NoProposal = 2,
    TooEarly = 3,
    WrongProposal = 4,
    InvalidReserve = 5,
    DeadlineOverflow = 6,
    AboveAuthorizedFee = 7,
    NonceOverflow = 8,
    InvalidTarget = 9,
}

fn authorize_owner(env: &Env) {
    let owner: Address = env.storage().instance().get(&Key::Owner).unwrap();
    owner.require_auth();
}

fn schedule(env: &Env) -> Result<(u64, u64), FeeError> {
    let executable_at = env.ledger().timestamp().checked_add(DELAY_SECONDS)
        .ok_or(FeeError::DeadlineOverflow)?;
    let storage = env.storage().instance();
    let nonce = storage.get::<_, u64>(&Key::Nonce).unwrap_or(0)
        .checked_add(1).ok_or(FeeError::NonceOverflow)?;
    storage.set(&Key::Nonce, &nonce);
    Ok((nonce, executable_at))
}

#[contract]
pub struct LaunchFees;

#[contractimpl]
impl LaunchFees {
    pub fn __constructor(env: Env, owner: Address, treasury: Address) {
        let storage = env.storage().instance();
        storage.set(&Key::Owner, &owner);
        storage.set(&Key::Treasury, &treasury);
        storage.set(&Key::CreationFee, &INITIAL_CREATION_FEE);
        storage.set(&Key::GraduationTarget, &INITIAL_GRADUATION_TARGET);
    }

    pub fn policy(env: Env, max_fee: i128) -> Result<FeePolicy, FeeError> {
        let storage = env.storage().instance();
        let creation_fee = storage.get(&Key::CreationFee).unwrap();
        if creation_fee > max_fee {
            return Err(FeeError::AboveAuthorizedFee);
        }
        Ok(FeePolicy {
            treasury: storage.get(&Key::Treasury).unwrap(),
            creation_fee,
        })
    }

    pub fn propose(env: Env, fee: i128) -> Result<CreationFeeChange, FeeError> {
        let storage = env.storage().instance();
        authorize_owner(&env);
        if !(0..=MAX_CREATION_FEE).contains(&fee) {
            return Err(FeeError::InvalidFee);
        }
        let (nonce, executable_at) = schedule(&env)?;
        let pending = CreationFeeChange { nonce, fee, executable_at };
        storage.set(&Key::Pending, &pending);
        FeeProposed { nonce, fee, executable_at }.publish(&env);
        Ok(pending)
    }

    pub fn cancel(env: Env) -> Result<(), FeeError> {
        let storage = env.storage().instance();
        authorize_owner(&env);
        let pending: CreationFeeChange = storage.get(&Key::Pending).ok_or(FeeError::NoProposal)?;
        storage.remove(&Key::Pending);
        FeeCanceled { nonce: pending.nonce, fee: pending.fee }.publish(&env);
        Ok(())
    }

    pub fn execute(env: Env, expected: CreationFeeChange) -> Result<(), FeeError> {
        let storage = env.storage().instance();
        let pending: CreationFeeChange = storage.get(&Key::Pending).ok_or(FeeError::NoProposal)?;
        if pending != expected {
            return Err(FeeError::WrongProposal);
        }
        if env.ledger().timestamp() < pending.executable_at {
            return Err(FeeError::TooEarly);
        }
        let previous = storage.get(&Key::CreationFee).unwrap();
        storage.set(&Key::CreationFee, &pending.fee);
        storage.remove(&Key::Pending);
        FeeChanged { nonce: pending.nonce, previous, current: pending.fee }.publish(&env);
        Ok(())
    }

    pub fn graduation_amounts(reserve: i128) -> Result<GraduationAmounts, FeeError> {
        if reserve <= 0 {
            return Err(FeeError::InvalidReserve);
        }
        let fee = reserve / GRADUATION_FEE_DIVISOR;
        Ok(GraduationAmounts { fee, pool_xlm: reserve - fee })
    }

    /// New launches snapshot this value when the creator authorizes the launch.
    pub fn graduation_target(env: Env) -> i128 {
        env.storage().instance().get(&Key::GraduationTarget)
            .unwrap_or(INITIAL_GRADUATION_TARGET)
    }

    pub fn propose_target(env: Env, target: i128) -> Result<GraduationTargetChange, FeeError> {
        authorize_owner(&env);
        if !(XLM..=MAX_GRADUATION_TARGET).contains(&target) {
            return Err(FeeError::InvalidTarget);
        }
        let (nonce, executable_at) = schedule(&env)?;
        let pending = GraduationTargetChange { nonce, target, executable_at };
        env.storage().instance().set(&Key::PendingTarget, &pending);
        TargetProposed { nonce, target, executable_at }.publish(&env);
        Ok(pending)
    }

    pub fn cancel_target(env: Env) -> Result<(), FeeError> {
        authorize_owner(&env);
        let storage = env.storage().instance();
        let pending: GraduationTargetChange = storage.get(&Key::PendingTarget)
            .ok_or(FeeError::NoProposal)?;
        storage.remove(&Key::PendingTarget);
        TargetCanceled { nonce: pending.nonce, target: pending.target }.publish(&env);
        Ok(())
    }

    pub fn execute_target(env: Env, expected: GraduationTargetChange) -> Result<(), FeeError> {
        let storage = env.storage().instance();
        let pending: GraduationTargetChange = storage.get(&Key::PendingTarget)
            .ok_or(FeeError::NoProposal)?;
        if pending != expected {
            return Err(FeeError::WrongProposal);
        }
        if env.ledger().timestamp() < pending.executable_at {
            return Err(FeeError::TooEarly);
        }
        let previous = Self::graduation_target(env.clone());
        storage.set(&Key::GraduationTarget, &pending.target);
        storage.remove(&Key::PendingTarget);
        TargetChanged { nonce: pending.nonce, previous, current: pending.target }.publish(&env);
        Ok(())
    }
}

#[cfg(test)]
mod test;
