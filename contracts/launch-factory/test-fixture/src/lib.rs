#![no_std]
//! Constructor fixture for verifying the factory before trading contracts exist.

use soroban_sdk::{contract, contractimpl, symbol_short, Env, Val};

#[contract]
pub struct MarketFixture;

#[contractimpl]
impl MarketFixture {
    pub fn __constructor(env: Env, config: Val) {
        env.storage().instance().set(&symbol_short!("config"), &config);
    }

    pub fn config(env: Env) -> Val {
        env.storage().instance().get(&symbol_short!("config")).unwrap()
    }
}
