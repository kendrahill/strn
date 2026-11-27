use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    IntoVal,
};

fn setup(env: &Env) -> (Address, Address, Address, Address) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let second = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(Address::generate(env)).address();
    let vault = env.register(StakingVault, (&token,));
    let client = StellarAssetClient::new(env, &token);
    client.mint(&owner, &1_000);
    client.mint(&second, &1_000);
    (token, vault, owner, second)
}

fn set_day(env: &Env, day: u64) {
    env.ledger().set_timestamp(day * DAY_SECONDS);
}

fn roll_to(client: &StakingVaultClient, target: u64) {
    while client.pool().day < target {
        client.roll(&MAX_ROLL_DAYS);
    }
}

fn assert_total(vault: &StakingVaultClient, first: &Address, second: &Address) {
    let pool = vault.pool();
    let a = vault.position(first);
    let b = vault.position(second);
    assert_eq!(a.day, pool.day);
    assert_eq!(b.day, pool.day);
    assert_eq!(pool.scaled_power, a.power() + b.power());
    assert_eq!(vault.day_power(&pool.day), Some(pool.scaled_power));
}

#[test]
fn next_day_positions_keep_weighted_age_and_global_power() {
    let env = Env::default();
    let (_, id, first, second) = setup(&env);
    let vault = StakingVaultClient::new(&env, &id);
    vault.queue_stake(&first, &100);
    vault.queue_stake(&second, &73);
    assert_eq!(vault.position(&first).principal, 0);
    assert_eq!(vault.pool().scaled_power, 0);

    set_day(&env, 1);
    roll_to(&vault, 1);
    assert_total(&vault, &first, &second);
    assert_eq!(vault.position(&first).power(), 100 * LOYALTY_DAYS);

    set_day(&env, 90);
    roll_to(&vault, 90);
    vault.queue_stake(&first, &150);
    vault.cancel_deposit(&first, &50);
    vault.queue_exit(&second, &17);
    vault.queue_stake(&second, &11);
    assert_eq!(vault.position(&first).principal, 100);

    set_day(&env, 91);
    roll_to(&vault, 91);
    assert_total(&vault, &first, &second);
    assert_eq!(vault.position(&first).power(), 250 * LOYALTY_DAYS);
    assert_eq!(vault.checkpoint(&first, &91).unwrap().age_credit, 9_000);
    assert_eq!(vault.position(&second).collectible, 17);

    set_day(&env, 135);
    roll_to(&vault, 135);
    vault.queue_exit(&first, &50);
    assert_eq!(vault.position(&first).principal, 200);
    set_day(&env, 136);
    roll_to(&vault, 136);
    assert_total(&vault, &first, &second);
    assert_eq!(vault.position(&first).power(), 225 * LOYALTY_DAYS);

    set_day(&env, 195);
    roll_to(&vault, 195);
    assert_total(&vault, &first, &second);
    assert_eq!(vault.position(&second).age_credit, 67 * LOYALTY_DAYS - 52);
    set_day(&env, 196);
    roll_to(&vault, 196);
    assert_total(&vault, &first, &second);
    assert_eq!(vault.position(&second).age_credit, 67 * LOYALTY_DAYS);

    set_day(&env, 226);
    roll_to(&vault, 226);
    assert_total(&vault, &first, &second);
    assert_eq!(vault.position(&first).power(), 300 * LOYALTY_DAYS);
    set_day(&env, 227);
    roll_to(&vault, 227);
    assert_total(&vault, &first, &second);
    assert_eq!(vault.pool().daily_increment, 0);
}

#[test]
fn exits_are_owner_bound_and_collectible_after_dormancy_without_pool_catchup() {
    let env = Env::default();
    let (asset, id, owner, _) = setup(&env);
    let vault = StakingVaultClient::new(&env, &id);
    let token = TokenClient::new(&env, &asset);
    vault.queue_stake(&owner, &100);
    vault.queue_stake(&owner, &13);
    vault.cancel_deposit(&owner, &13);
    assert_eq!(vault.principal_liability(), 100);
    assert_eq!(vault.try_queue_exit(&owner, &1), Err(Ok(VaultError::InsufficientPrincipal)));
    set_day(&env, 1);
    roll_to(&vault, 1);
    vault.queue_exit(&owner, &100);
    assert_eq!(vault.try_collect_exit(&owner), Err(Ok(VaultError::NoPrincipalToCollect)));
    assert_eq!(vault.try_queue_exit(&owner, &1), Err(Ok(VaultError::InsufficientPrincipal)));

    set_day(&env, 366);
    env.set_auths(&[]);
    assert!(vault.try_collect_exit(&owner).is_err());
    let auth = MockAuth {
        address: &owner,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "collect_exit",
            args: (&owner,).into_val(&env),
            sub_invokes: &[],
        },
    };
    assert_eq!(vault.mock_auths(&[auth]).collect_exit(&owner), 100);
    assert_eq!(vault.pool().day, 1);
    assert_eq!(vault.principal_liability(), 0);
    assert_eq!(token.balance(&owner), 1_000);
    assert_eq!(token.balance(&id), 0);
    assert_eq!(vault.position(&owner).age_credit, 0);
    env.mock_all_auths();
    assert_eq!(vault.try_collect_exit(&owner), Err(Ok(VaultError::NoPrincipalToCollect)));
    vault.queue_stake(&owner, &1);
    vault.cancel_deposit(&owner, &1);
    assert_eq!(vault.principal_liability(), 0);
    assert_eq!(vault.roll(&16).day, 17);
}

extern crate std;

#[contracttype]
struct LaunchRoles {
    curve: Address,
    amm_inventory: Address,
    reward_controller: Address,
    reward_vault: Address,
}

#[contracttype]
struct TokenMetadata {
    name: soroban_sdk::String,
    symbol: soroban_sdk::String,
    description: soroban_sdk::String,
    image_uri: Option<soroban_sdk::String>,
    image_sha256: Option<soroban_sdk::BytesN<32>>,
    website_uri: Option<soroban_sdk::String>,
}

#[test]
#[ignore = "Set SATURN_PROJECT_TOKEN_WASM and run explicitly for cross-contract verification"]
fn project_token_wasm_mints_and_claims_rewards_through_the_vault() {
    let wasm = std::fs::read(std::env::var("SATURN_PROJECT_TOKEN_WASM").unwrap()).unwrap();
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    let curve = Address::generate(&env);
    let id = Address::generate(&env);
    let roles = LaunchRoles {
        curve: curve.clone(),
        amm_inventory: Address::generate(&env),
        reward_controller: id.clone(),
        reward_vault: id.clone(),
    };
    let metadata = TokenMetadata {
        name: soroban_sdk::String::from_str(&env, "Saturn Test"),
        symbol: soroban_sdk::String::from_str(&env, "TEST"),
        description: soroban_sdk::String::from_str(&env, ""),
        image_uri: None,
        image_sha256: None,
        website_uri: None,
    };
    let asset = env.register(wasm.as_slice(), (&first, &metadata, false, &roles));
    if let Ok(path) = std::env::var("SATURN_STAKING_VAULT_WASM") {
        let vault_wasm = std::fs::read(path).unwrap();
        env.register_at(&id, vault_wasm.as_slice(), (&asset,));
    } else {
        env.register_at(&id, StakingVault, (&asset,));
    }
    let vault = StakingVaultClient::new(&env, &id);
    let token = TokenClient::new(&env, &asset);
    token.transfer(&curve, &first, &100);
    token.transfer(&curve, &second, &200);
    token.burn(&curve, &61);
    vault.queue_stake(&first, &100);
    vault.queue_stake(&second, &200);
    assert_eq!(vault.principal_liability(), 300);
    assert_eq!(vault.try_settle_rewards(), Err(Ok(VaultError::EpochNotReady)));

    // The real vault must authorize the pinned token release by immediate
    // contract invocation without any blanket authorization mocks.
    env.set_auths(&[]);
    set_day(&env, 1);
    assert_eq!(vault.settle_rewards().minted, 0);
    set_day(&env, 2);
    assert_eq!(vault.try_settle_rewards(), Err(Ok(VaultError::EpochNotReady)));
    vault.roll(&2);
    let first_epoch = vault.settle_rewards();
    assert_eq!(first_epoch.power, 300 * LOYALTY_DAYS);
    assert_eq!(first_epoch.recycled, 2);
    assert_eq!(first_epoch.minted, 300_000_000 * 10_000_000 / 3_650 + 2);
    assert_eq!(vault.try_settle_rewards(), Err(Ok(VaultError::EpochNotReady)));
    assert_eq!(token.balance(&id), 300 + first_epoch.minted);

    assert!(vault.try_claim(&first, &1).is_err());
    let auth = MockAuth {
        address: &first,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "claim",
            args: (&first, 1u32).into_val(&env),
            sub_invokes: &[],
        },
    };
    let first_claim = vault.mock_auths(&[auth]).claim(&first, &1);
    assert_eq!(first_claim.paid, first_epoch.minted / 3);
    env.mock_all_auths();
    assert_eq!(vault.claim(&first, &1).paid, 0);

    vault.queue_exit(&first, &100);
    set_day(&env, 3);
    assert_eq!(vault.collect_exit(&first), 100);
    assert_eq!(vault.principal_liability(), 200);
    assert_eq!(token.balance(&id), 200 + vault.reward_state().escrowed);

    set_day(&env, 4);
    vault.roll(&2);
    let second_epoch = vault.settle_rewards();
    let third_epoch = vault.settle_rewards();
    assert_eq!(third_epoch.power, vault.position(&second).power() - 200);
    let first_again = vault.claim(&first, &16);
    assert_eq!(first_again.paid, (first_epoch.minted + second_epoch.minted) / 3 - first_claim.paid);
    let second_claim = vault.claim(&second, &16);
    assert_eq!(second_claim.paid, (first_epoch.minted + second_epoch.minted) * 2 / 3 + third_epoch.minted);
    assert_eq!(vault.claim(&second, &16).paid, 0);
    assert_eq!(token.balance(&id), 200 + vault.reward_state().escrowed);
}
