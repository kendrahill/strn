use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    token::TokenClient,
    IntoVal,
};

fn launch(env: &Env) -> (Address, Address, LaunchRoles) {
    let creator = Address::generate(env);
    let roles = LaunchRoles {
        curve: Address::generate(env),
        amm_inventory: Address::generate(env),
        reward_controller: Address::generate(env),
        reward_vault: Address::generate(env),
    };
    let fields = TokenMetadata {
        name: String::from_str(env, "Test Project"),
        symbol: String::from_str(env, "TEST"),
        description: String::from_str(env, ""),
        image_uri: None,
        image_sha256: None,
        website_uri: None,
    };
    env.mock_all_auths();
    let id = env.register(ProjectToken, (&creator, &fields, false, &roles));
    env.set_auths(&[]);
    (id, creator, roles)
}

fn assert_cap(client: &ProjectTokenClient) {
    let state = client.supply();
    assert_eq!(state.outstanding + state.unused_initial + state.unused_recycled, client.cap());
    assert!(state.matured_recycled <= state.unused_recycled);
}

#[test]
fn metadata_and_wallet_operations_use_the_deployed_token() {
    let env = Env::default();
    let (id, creator, roles) = launch(&env);
    let contract = ProjectTokenClient::new(&env, &id);
    let sep = TokenClient::new(&env, &id);
    assert_eq!(sep.balance(&roles.curve), 400_000_000 * UNIT);
    assert_eq!(sep.balance(&roles.amm_inventory), 300_000_000 * UNIT);
    assert_eq!(sep.balance(&creator), 0);
    assert_eq!(sep.decimals(), 7);
    assert_eq!(sep.symbol(), String::from_str(&env, "TEST"));
    assert!(contract.try_lock_metadata(&0).is_err());
    let mut fields = contract.metadata().metadata;
    fields.name = String::from_str(&env, "New Name");
    assert!(contract.try_update_metadata(&0, &fields).is_err());
    env.mock_all_auths();
    contract.update_metadata(&0, &fields);
    contract.lock_metadata(&1);
    assert_eq!(sep.name(), fields.name);
    assert_eq!(contract.try_update_metadata(&1, &fields), Err(Ok(MetadataError::Locked)));

    sep.transfer(&roles.curve, &creator, &50);
    assert_eq!(sep.balance(&creator), 50);
    assert_eq!(contract.supply().unused_recycled, 0);
    assert_cap(&contract);
}

#[test]
fn delegated_burn_charges_an_allowance_once_and_credits_recycling_once() {
    let env = Env::default();
    let (id, creator, roles) = launch(&env);
    let contract = ProjectTokenClient::new(&env, &id);
    env.mock_all_auths();
    contract.approve(&roles.curve, &creator, &70, &200);
    contract.burn_from(&creator, &roles.curve, &50);
    assert_eq!(contract.allowance(&roles.curve, &creator), 20);
    assert_eq!(contract.supply().unused_recycled, 50);
    assert_eq!(contract.burned_on(&0), 50);
    assert_eq!(contract.try_burn_from(&creator, &roles.curve, &21), Err(Ok(TokenError::InsufficientAllowance.into())));
    env.ledger().set_sequence_number(201);
    assert_eq!(contract.allowance(&roles.curve, &creator), 0);
    assert_eq!(contract.try_burn_from(&creator, &roles.curve, &1), Err(Ok(TokenError::InsufficientAllowance.into())));
    assert_eq!(contract.supply().unused_recycled, 50);
    assert_cap(&contract);
}

#[test]
fn only_the_controller_can_release_closed_days_to_the_pinned_vault() {
    let env = Env::default();
    let (id, _, roles) = launch(&env);
    let contract = ProjectTokenClient::new(&env, &id);
    env.mock_all_auths();
    contract.burn(&roles.curve, &50);
    assert_eq!(contract.try_release_day(&0, &true, &0), Err(Ok(TokenError::InvalidRewardDay)));
    env.ledger().set_timestamp(DAY_SECONDS);
    assert_eq!(contract.try_release_day(&0, &true, &1), Err(Ok(TokenError::ExcessRecycledRewards)));
    env.set_auths(&[]);
    assert!(contract.try_release_day(&0, &true, &0).is_err());
    contract.mock_auths(&[MockAuth {
        address: &roles.reward_controller,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "release_day",
            args: (0u64, true, 0i128).into_val(&env),
            sub_invokes: &[],
        },
    }]).release_day(&0, &true, &0);
    assert_eq!(contract.balance(&roles.reward_vault), INITIAL_REWARDS / 3_650);
    assert_eq!(contract.supply().matured_recycled, 0);
    assert_cap(&contract);

    env.mock_all_auths();
    env.ledger().set_timestamp(2 * DAY_SECONDS);
    assert_eq!(contract.try_release_day(&0, &true, &0), Err(Ok(TokenError::InvalidRewardDay)));
    assert_eq!(contract.try_release_day(&1, &false, &50), Err(Ok(TokenError::ExcessRecycledRewards)));
    contract.release_day(&1, &false, &0);
    assert_eq!(contract.supply().eligible_days, 1);
    assert_eq!(contract.supply().matured_recycled, 50);
    env.ledger().set_timestamp(3 * DAY_SECONDS);
    contract.release_day(&2, &true, &50);
    assert_eq!(contract.balance(&roles.reward_vault), INITIAL_REWARDS * 2 / 3_650 + 50);
    assert_eq!(contract.supply().unused_recycled, 0);
    assert_cap(&contract);
}

#[test]
fn exactly_300_million_are_released_across_the_initial_schedule() {
    let env = Env::default();
    let (id, _, roles) = launch(&env);
    let contract = ProjectTokenClient::new(&env, &id);
    env.mock_all_auths();
    // Each release is modeled as a separate transaction with a fresh budget.
    for epoch in 0..=u64::from(REWARD_DAYS) {
        env.cost_estimate().budget().reset_default();
        env.ledger().set_timestamp((epoch + 1) * DAY_SECONDS);
        contract.release_day(&epoch, &true, &0);
    }
    assert_eq!(contract.balance(&roles.reward_vault), INITIAL_REWARDS);
    assert_eq!(contract.supply().eligible_days, REWARD_DAYS);
    assert_eq!(contract.supply().unused_initial, 0);
    assert_cap(&contract);
}
