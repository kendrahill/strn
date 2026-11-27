use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke}, IntoVal};

#[test]
fn creation_fee_requires_governance_and_honors_the_timelock_and_creator_limit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let treasury = Address::generate(&env);
    let id = env.register(LaunchFees, (&owner, &treasury));
    let contract = LaunchFeesClient::new(&env, &id);
    assert_eq!(contract.policy(&INITIAL_CREATION_FEE), FeePolicy {
        treasury, creation_fee: INITIAL_CREATION_FEE,
    });
    assert_eq!(contract.try_policy(&(INITIAL_CREATION_FEE - 1)), Err(Ok(FeeError::AboveAuthorizedFee)));
    assert!(contract.try_propose(&(125 * XLM)).is_err());
    let fee = 125 * XLM;
    let proposal = contract.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "propose",
            args: (fee,).into_val(&env),
            sub_invokes: &[],
        },
    }]).propose(&fee);
    assert_eq!(contract.try_execute(&proposal), Err(Ok(FeeError::TooEarly)));
    env.ledger().set_timestamp(proposal.executable_at);
    contract.execute(&proposal);
    assert_eq!(contract.policy(&fee).creation_fee, fee);
    assert_eq!(contract.try_execute(&proposal), Err(Ok(FeeError::NoProposal)));
}

#[test]
fn stale_canceled_and_out_of_range_changes_are_rejected() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let id = env.register(LaunchFees, (&owner, &Address::generate(&env)));
    let contract = LaunchFeesClient::new(&env, &id);
    env.mock_all_auths();
    assert_eq!(contract.try_propose(&(MAX_CREATION_FEE + 1)), Err(Ok(FeeError::InvalidFee)));
    let prior = contract.propose(&0);
    contract.cancel();
    let replacement = contract.propose(&0);
    env.ledger().set_timestamp(replacement.executable_at);
    assert_eq!(contract.try_execute(&prior), Err(Ok(FeeError::WrongProposal)));
    contract.execute(&replacement);
    assert_eq!(contract.policy(&0).creation_fee, 0);
}

#[test]
fn graduation_deducts_fifty_basis_points_from_the_actual_reserve() {
    let env = Env::default();
    let id = env.register(LaunchFees, (&Address::generate(&env), &Address::generate(&env)));
    let contract = LaunchFeesClient::new(&env, &id);
    assert_eq!(contract.graduation_amounts(&(30_000 * XLM)), GraduationAmounts {
        fee: 150 * XLM, pool_xlm: 29_850 * XLM,
    });
    let dust_reserve = 30_000 * XLM - 199;
    let amounts = contract.graduation_amounts(&dust_reserve);
    assert_eq!(amounts.fee, 150 * XLM - 1);
    assert_eq!(amounts.fee + amounts.pool_xlm, dust_reserve);
    assert_eq!(contract.try_graduation_amounts(&0), Err(Ok(FeeError::InvalidReserve)));
}

#[test]
fn governance_can_lower_the_target_independently_of_the_launch_fee() {
    let env = Env::default();
    let id = env.register(LaunchFees, (&Address::generate(&env), &Address::generate(&env)));
    let contract = LaunchFeesClient::new(&env, &id);
    assert_eq!(contract.graduation_target(), 30_000 * XLM);
    assert!(contract.try_propose_target(&(15_000 * XLM)).is_err());
    env.mock_all_auths();
    assert_eq!(contract.try_propose_target(&0), Err(Ok(FeeError::InvalidTarget)));
    let fee = contract.propose(&(90 * XLM));
    let canceled = contract.propose_target(&(18_000 * XLM));
    contract.cancel_target();
    assert_eq!(contract.try_execute_target(&canceled), Err(Ok(FeeError::NoProposal)));
    let target = contract.propose_target(&(15_000 * XLM));
    assert_eq!(contract.try_execute_target(&canceled), Err(Ok(FeeError::WrongProposal)));
    assert_eq!(contract.try_execute_target(&target), Err(Ok(FeeError::TooEarly)));
    env.ledger().set_timestamp(target.executable_at);
    contract.execute_target(&target);
    assert_eq!(contract.graduation_target(), 15_000 * XLM);
    assert_eq!(contract.policy(&INITIAL_CREATION_FEE).creation_fee, INITIAL_CREATION_FEE);
    contract.execute(&fee);
    assert_eq!(contract.policy(&(90 * XLM)).creation_fee, 90 * XLM);
}

use saturn_token_metadata::{self as metadata, MetadataError, MetadataState, TokenMetadata};
use soroban_sdk::String;

#[contract]
struct MetadataHarness;

#[contractimpl]
impl MetadataHarness {
    pub fn __constructor(env: Env, creator: Address, value: TokenMetadata, locked: bool) {
        metadata::initialize(&env, creator, value, locked).unwrap();
    }

    pub fn get(env: Env) -> MetadataState {
        metadata::get(&env)
    }

    pub fn update(env: Env, revision: u64, value: TokenMetadata) -> Result<(), MetadataError> {
        metadata::update(&env, revision, value)
    }

    pub fn lock(env: Env, revision: u64) -> Result<(), MetadataError> {
        metadata::lock(&env, revision)
    }
}

#[test]
fn creator_can_edit_then_permanently_lock_token_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let value = TokenMetadata {
        name: String::from_str(&env, "Saturn Project"),
        symbol: String::from_str(&env, "SP"),
        description: String::from_str(&env, ""),
        image_uri: None,
        image_sha256: None,
        website_uri: None,
    };
    let id = env.register(MetadataHarness, (&creator, &value, false));
    let contract = MetadataHarnessClient::new(&env, &id);
    env.set_auths(&[]);
    let mut edited = value.clone();
    edited.description = String::from_str(&env, "Updated details");
    assert!(contract.try_update(&0, &edited).is_err());
    assert!(contract.try_lock(&0).is_err());
    contract.mock_auths(&[MockAuth {
        address: &creator,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "update",
            args: (0u64, &edited).into_val(&env),
            sub_invokes: &[],
        },
    }]).update(&0, &edited);
    env.mock_all_auths();
    assert_eq!(contract.try_lock(&0), Err(Ok(MetadataError::StaleRevision)));
    let mut invalid = edited.clone();
    invalid.name = String::from_str(&env, " ");
    assert_eq!(contract.try_update(&1, &invalid), Err(Ok(MetadataError::InvalidMetadata)));
    edited.image_uri = Some(String::from_str(&env, "https://example.com/token.png"));
    contract.update(&1, &edited);
    assert_eq!(contract.try_lock(&2), Err(Ok(MetadataError::ImageDigestRequired)));
    edited.image_sha256 = Some(soroban_sdk::BytesN::from_array(&env, &[1; 32]));
    contract.update(&2, &edited);
    contract.lock(&3);
    assert_eq!(contract.try_update(&3, &value), Err(Ok(MetadataError::Locked)));
    assert_eq!(contract.get(), MetadataState { creator, metadata: edited, revision: 3, locked: true });
    let locked_id = env.register(MetadataHarness, (&contract.get().creator, &value, true));
    assert!(MetadataHarnessClient::new(&env, &locked_id).get().locked);
}
