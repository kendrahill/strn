#![no_std]

use saturn_token_metadata::TokenMetadata;
use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    token::TokenClient, xdr::ToXdr, Address, BytesN, ContractExecutable, Env,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Template {
    pub token_wasm: BytesN<32>,
    pub vault_wasm: BytesN<32>,
    pub curve_wasm: BytesN<32>,
    pub pool_wasm: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryConfig {
    pub governance: Address,
    pub xlm: Address,
    pub strn_rewards: Address,
    pub foundation: Address,
    pub template: Template,
}

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
pub struct LaunchRequest {
    pub salt: BytesN<32>,
    pub max_creation_fee: i128,
    pub expected_target: i128,
    pub lock_metadata: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchRecord {
    pub creator: Address,
    pub addresses: LaunchAddresses,
    pub target: i128,
    pub creation_fee: i128,
    pub treasury: Address,
}

#[contractevent]
pub struct LaunchCreated {
    #[topic]
    pub creator: Address,
    #[topic]
    pub token: Address,
    pub salt: BytesN<32>,
    pub launch: LaunchRecord,
}

#[contracttype]
enum Key {
    Config,
    Launch(Address, BytesN<32>),
    Token(Address),
}

#[contracttype]
struct LaunchRoles {
    curve: Address,
    amm_inventory: Address,
    reward_controller: Address,
    reward_vault: Address,
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
pub struct FeePolicy {
    pub treasury: Address,
    pub creation_fee: i128,
}

#[contractclient(name = "GovernanceClient")]
#[allow(dead_code)]
trait Governance {
    fn graduation_target(env: Env) -> i128;
    fn policy(env: Env, max_fee: i128) -> FeePolicy;
}

#[contracttype]
#[derive(Clone, Copy)]
enum Role {
    Token,
    Vault,
    Curve,
    Pool,
}

fn child_salt(env: &Env, creator: &Address, salt: &BytesN<32>, role: Role) -> BytesN<32> {
    let preimage = (creator.clone(), salt.clone(), role).to_xdr(env);
    env.crypto().sha256(&preimage).to_bytes()
}

fn predict(env: &Env, creator: &Address, salt: &BytesN<32>) -> LaunchAddresses {
    let address = |role| {
        env.deployer().with_current_contract(child_salt(env, creator, salt, role)).deployed_address()
    };
    LaunchAddresses {
        token: address(Role::Token),
        vault: address(Role::Vault),
        curve: address(Role::Curve),
        pool: address(Role::Pool),
    }
}

fn deploy<A>(
    env: &Env,
    creator: &Address,
    salt: &BytesN<32>,
    role: Role,
    wasm: BytesN<32>,
    args: A,
) where
    A: soroban_sdk::ConstructorArgs,
{
    env.deployer().with_current_contract(child_salt(env, creator, salt, role))
        .deploy_contract(ContractExecutable::Wasm(wasm), args);
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FactoryError {
    DuplicateLaunch = 1,
    TargetChanged = 2,
    InvalidFeeCeiling = 3,
    FeeNotReceived = 4,
}

#[contract]
pub struct LaunchFactory;

#[contractimpl]
impl LaunchFactory {
    pub fn __constructor(env: Env, config: FactoryConfig) {
        env.storage().instance().set(&Key::Config, &config);
    }

    pub fn config(env: Env) -> FactoryConfig {
        env.storage().instance().get(&Key::Config).unwrap()
    }

    pub fn predict_launch(env: Env, creator: Address, salt: BytesN<32>) -> LaunchAddresses {
        predict(&env, &creator, &salt)
    }

    pub fn resolve_launch(env: Env, creator: Address, salt: BytesN<32>) -> Option<LaunchRecord> {
        env.storage().persistent().get(&Key::Launch(creator, salt))
    }

    pub fn resolve_token(env: Env, token: Address) -> Option<LaunchRecord> {
        let (creator, salt): (Address, BytesN<32>) = env.storage().persistent().get(&Key::Token(token))?;
        Self::resolve_launch(env, creator, salt)
    }

    pub fn create_launch(
        env: Env,
        creator: Address,
        metadata: TokenMetadata,
        request: LaunchRequest,
    ) -> Result<LaunchRecord, FactoryError> {
        creator.require_auth();
        let key = Key::Launch(creator.clone(), request.salt.clone());
        if env.storage().persistent().has(&key) {
            return Err(FactoryError::DuplicateLaunch);
        }
        if request.max_creation_fee < 0 {
            return Err(FactoryError::InvalidFeeCeiling);
        }
        let config = Self::config(env.clone());
        let governance = GovernanceClient::new(&env, &config.governance);
        let target = governance.graduation_target();
        if target != request.expected_target {
            return Err(FactoryError::TargetChanged);
        }
        let policy = governance.policy(&request.max_creation_fee);
        if policy.creation_fee > 0 {
            let xlm = TokenClient::new(&env, &config.xlm);
            let before = xlm.balance(&policy.treasury);
            xlm.transfer(&creator, &policy.treasury, &policy.creation_fee);
            if creator != policy.treasury
                && xlm.balance(&policy.treasury) - before != policy.creation_fee
            {
                return Err(FactoryError::FeeNotReceived);
            }
        }

        let ids = predict(&env, &creator, &request.salt);
        let roles = LaunchRoles {
            curve: ids.curve.clone(),
            // The curve's closing transaction releases the separate seed
            // allocation. Its trade inventory must use a separate counter.
            amm_inventory: ids.curve.clone(),
            reward_controller: ids.vault.clone(),
            reward_vault: ids.vault.clone(),
        };
        deploy(&env, &creator, &request.salt, Role::Token, config.template.token_wasm,
            (creator.clone(), metadata, request.lock_metadata, roles));
        deploy(&env, &creator, &request.salt, Role::Vault, config.template.vault_wasm,
            (ids.token.clone(),));
        let market = MarketConfig {
            creator: creator.clone(), addresses: ids.clone(), xlm: config.xlm,
            treasury: policy.treasury.clone(), strn_rewards: config.strn_rewards,
            foundation: config.foundation, target,
        };
        deploy(&env, &creator, &request.salt, Role::Pool, config.template.pool_wasm, (&market,));
        deploy(&env, &creator, &request.salt, Role::Curve, config.template.curve_wasm, (&market,));

        let launch = LaunchRecord {
            creator: creator.clone(), addresses: ids.clone(), target,
            creation_fee: policy.creation_fee, treasury: policy.treasury,
        };
        let storage = env.storage().persistent();
        storage.set(&key, &launch);
        storage.set(&Key::Token(ids.token.clone()), &(creator.clone(), request.salt.clone()));
        LaunchCreated { creator, token: ids.token, salt: request.salt, launch: launch.clone() }.publish(&env);
        Ok(launch)
    }
}

#[cfg(test)]
mod test;
