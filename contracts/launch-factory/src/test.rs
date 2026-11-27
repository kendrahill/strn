extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Bytes, IntoVal, Map, String, Symbol, TryFromVal, Val,
};

fn wasm(name: &str) -> std::vec::Vec<u8> {
    let directory = std::env::var("SATURN_WASM_DIR").unwrap();
    let path = std::path::Path::new(&directory).join(name);
    std::fs::read(path).unwrap()
}

fn upload(env: &Env, name: &str) -> BytesN<32> {
    env.deployer().upload_contract_wasm(Bytes::from_slice(env, &wasm(name)))
}

fn metadata(env: &Env) -> TokenMetadata {
    TokenMetadata {
        name: String::from_str(env, "Test Project"),
        symbol: String::from_str(env, "TEST"),
        description: String::from_str(env, ""),
        image_uri: None,
        image_sha256: None,
        website_uri: None,
    }
}

#[test]
fn address_prediction_separates_each_role_creator_and_salt() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0; 32]);
    let cfg = FactoryConfig {
        governance: Address::generate(&env),
        xlm: Address::generate(&env),
        strn_rewards: Address::generate(&env),
        foundation: Address::generate(&env),
        template: Template {
            token_wasm: hash.clone(), vault_wasm: hash.clone(),
            curve_wasm: hash.clone(), pool_wasm: hash.clone(),
        },
    };
    let id = env.register(LaunchFactory, (&cfg,));
    let client = LaunchFactoryClient::new(&env, &id);
    let ids = client.predict_launch(&owner, &hash);
    assert_eq!(ids, client.predict_launch(&owner, &hash));
    let distinct = [&ids.token, &ids.vault, &ids.curve, &ids.pool];
    for i in 0..distinct.len() {
        assert!(!distinct[..i].contains(&distinct[i]));
    }
    assert_ne!(ids.token, client.predict_launch(&other, &hash).token);
    assert_ne!(ids.token, client.predict_launch(&owner, &BytesN::from_array(&env, &[1; 32])).token);
}

#[contracttype]
#[derive(Clone)]
struct TargetProposal {
    nonce: u64,
    target: i128,
    executable_at: u64,
}

#[contractclient(name = "TargetTestClient")]
#[allow(dead_code)]
trait TargetTest {
    fn propose_target(env: Env, target: i128) -> TargetProposal;
    fn execute_target(env: Env, expected: TargetProposal);
}

#[test]
#[ignore = "Build the project, vault, governance, market fixture and factory WASMs and set SATURN_WASM_DIR"]
fn factory_deploys_the_bundle_and_rolls_back_fees_on_failure() {
    const XLM: i128 = 10_000_000;
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let owner = Address::generate(&env);
    let creator = Address::generate(&env);
    let treasury = Address::generate(&env);
    let xlm = env.register_stellar_asset_contract_v2(owner.clone()).address();
    StellarAssetClient::new(&env, &xlm).mint(&creator, &(500 * XLM));
    let governance = env.register(wasm("saturn_launch_fees.wasm").as_slice(), (&owner, &treasury));
    let market = upload(&env, "saturn_market_test_fixture.wasm");
    let cfg = FactoryConfig {
        governance: governance.clone(), xlm: xlm.clone(), strn_rewards: Address::generate(&env),
        foundation: Address::generate(&env),
        template: Template {
            token_wasm: upload(&env, "saturn_project_token.wasm"),
            vault_wasm: upload(&env, "saturn_staking_vault.wasm"),
            curve_wasm: market.clone(), pool_wasm: market,
        },
    };
    let id = env.register(wasm("saturn_launch_factory.wasm").as_slice(), (&cfg,));
    let factory = LaunchFactoryClient::new(&env, &id);
    let xlm_client = TokenClient::new(&env, &xlm);
    let target = TargetTestClient::new(&env, &governance);
    let mut request = LaunchRequest {
        salt: BytesN::from_array(&env, &[41; 32]), max_creation_fee: 100 * XLM,
        expected_target: 30_000 * XLM, lock_metadata: true,
    };
    let predicted = factory.predict_launch(&creator, &request.salt);
    assert_eq!(factory.resolve_token(&predicted.token), None);

    let mut bad_cfg = cfg.clone();
    bad_cfg.template.curve_wasm = BytesN::from_array(&env, &[77; 32]);
    let bad_id = env.register(wasm("saturn_launch_factory.wasm").as_slice(), (&bad_cfg,));
    let bad_factory = LaunchFactoryClient::new(&env, &bad_id);
    let unlaunched = bad_factory.predict_launch(&creator, &request.salt);
    assert!(bad_factory.try_create_launch(&creator, &metadata(&env), &request).is_err());
    assert_eq!(bad_factory.resolve_token(&unlaunched.token), None);
    assert!(env.try_invoke_contract::<i128, soroban_sdk::Error>(
        &unlaunched.token, &soroban_sdk::symbol_short!("cap"), soroban_sdk::Vec::new(&env),
    ).is_err());
    assert_eq!(xlm_client.balance(&treasury), 0);

    env.set_auths(&[]);
    assert!(factory.try_create_launch(&creator, &metadata(&env), &request).is_err());
    env.mock_all_auths_allowing_non_root_auth();
    let mut bad_fields = metadata(&env);
    bad_fields.name = String::from_str(&env, "");
    assert!(factory.try_create_launch(&creator, &bad_fields, &request).is_err());
    assert_eq!(xlm_client.balance(&treasury), 0);
    assert_eq!(xlm_client.balance(&creator), 500 * XLM);
    assert_eq!(factory.resolve_launch(&creator, &request.salt), None);

    request.max_creation_fee -= 1;
    assert!(factory.try_create_launch(&creator, &metadata(&env), &request).is_err());
    assert_eq!(xlm_client.balance(&treasury), 0);
    request.max_creation_fee += 1;
    let record = factory.create_launch(&creator, &metadata(&env), &request);
    assert_eq!(record.addresses, predicted);
    assert_eq!(factory.resolve_token(&predicted.token), Some(record.clone()));
    assert_eq!(factory.resolve_launch(&creator, &request.salt), Some(record));
    assert_eq!(xlm_client.balance(&treasury), 100 * XLM);
    assert_eq!(TokenClient::new(&env, &predicted.token).balance(&predicted.curve), 700_000_000 * XLM);
    let staking_asset: Address = env.invoke_contract(&predicted.vault, &soroban_sdk::symbol_short!("asset"), soroban_sdk::Vec::new(&env));
    assert_eq!(staking_asset, predicted.token);
    let markets = MarketConfig {
        addresses: predicted.clone(), creator: creator.clone(), treasury: treasury.clone(),
        strn_rewards: cfg.strn_rewards.clone(), foundation: cfg.foundation.clone(),
        xlm: xlm.clone(), target: 30_000 * XLM,
    };
    for address in [&predicted.curve, &predicted.pool] {
        let observed: MarketConfig = env.invoke_contract(address, &soroban_sdk::symbol_short!("config"), soroban_sdk::Vec::new(&env));
        assert_eq!(observed, markets);
    }
    assert_eq!(factory.try_create_launch(&creator, &metadata(&env), &request), Err(Ok(FactoryError::DuplicateLaunch)));
    assert_eq!(xlm_client.balance(&treasury), 100 * XLM);

    let proposal = target.propose_target(&(15_000 * XLM));
    env.ledger().set_timestamp(proposal.executable_at);
    target.execute_target(&proposal);
    let original = request.salt.clone();
    request.salt = BytesN::from_array(&env, &[42; 32]);
    assert_eq!(factory.try_create_launch(&creator, &metadata(&env), &request), Err(Ok(FactoryError::TargetChanged)));
    assert_eq!(xlm_client.balance(&treasury), 100 * XLM);
    request.expected_target = 15_000 * XLM;
    assert_eq!(factory.create_launch(&creator, &metadata(&env), &request).target, 15_000 * XLM);
    assert_eq!(factory.resolve_launch(&creator, &original).unwrap().target, 30_000 * XLM);
    assert_eq!(xlm_client.balance(&treasury), 200 * XLM);
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct Graduation {
    dust: i128,
    gross_xlm: i128,
    fee: i128,
    pool_xlm: i128,
    pool_tokens: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct Trade {
    tokens: i128,
    xlm: i128,
    token_fee: i128,
    xlm_fee: i128,
    input: i128,
    output: i128,
    graduation: Option<Map<Symbol, i128>>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct Reserves {
    xlm: i128,
    tokens: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct LiquidityQuote {
    xlm: i128,
    tokens: i128,
    shares: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ShareSupply {
    total: i128,
    locked: i128,
}

#[contractclient(name = "MarketClient")]
#[allow(dead_code)]
trait Market {
    fn quote_curve(env: Env, buy: bool, tokens: i128) -> Trade;
    fn trade_curve(env: Env, trader: Address, buy: bool, tokens: i128,
        max_input: i128, min_output: i128, deadline: u64) -> Trade;
    fn seed(env: Env, xlm: i128, tokens: i128);
    fn reserves(env: Env) -> Option<Reserves>;
    fn quote_swap(env: Env, buy: bool, principal: i128) -> Trade;
    fn swap(env: Env, trader: Address, buy: bool, principal: i128,
        max_input: i128, min_output: i128, deadline: u64) -> Trade;
    fn share_supply(env: Env) -> Option<ShareSupply>;
    fn lp_balance(env: Env, owner: Address) -> i128;
    fn quote_deposit(env: Env, max_xlm: i128, max_tokens: i128) -> LiquidityQuote;
    fn deposit(env: Env, owner: Address, max_xlm: i128, max_tokens: i128,
        min_shares: i128, deadline: u64) -> LiquidityQuote;
    fn quote_withdraw(env: Env, shares: i128) -> LiquidityQuote;
    fn withdraw(env: Env, owner: Address, shares: i128, min_xlm: i128, min_tokens: i128,
        deadline: u64) -> LiquidityQuote;
}

#[contracttype]
#[derive(Clone, Debug)]
struct RewardEpoch {
    day: u64,
    power: i128,
    minted: i128,
    recycled: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
struct ClaimReceipt {
    paid: i128,
    next_day: u64,
    has_more: bool,
}

#[contractclient(name = "StakeClient")]
#[allow(dead_code)]
trait Stake {
    fn queue_stake(env: Env, owner: Address, amount: i128);
    fn principal_liability(env: Env) -> i128;
    fn roll(env: Env, max_days: u32) -> Val;
    fn settle_rewards(env: Env) -> RewardEpoch;
    fn claim(env: Env, owner: Address, max_days: u32) -> ClaimReceipt;
}

#[test]
#[ignore = "Build all factory dependencies plus saturn_project_market.wasm and set SATURN_WASM_DIR"]
fn bonding_curve_rolls_back_a_failed_close_and_seeds_a_working_amm() {
    const UNIT: i128 = 10_000_000;
    const HALF: i128 = 200_000_000 * UNIT;
    const SEED: i128 = 300_000_000 * UNIT;
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let trader = Address::generate(&env);
    let treasury = Address::generate(&env);
    let xlm = env.register_stellar_asset_contract_v2(admin.clone()).address();
    StellarAssetClient::new(&env, &xlm).mint(&creator, &(500 * UNIT));
    StellarAssetClient::new(&env, &xlm).mint(&trader, &(100_000 * UNIT));
    let governance = env.register(wasm("saturn_launch_fees.wasm").as_slice(), (&admin, &treasury));
    let market = upload(&env, "saturn_project_market.wasm");
    let mut config = FactoryConfig {
        governance, xlm: xlm.clone(), strn_rewards: Address::generate(&env),
        foundation: Address::generate(&env),
        template: Template {
            token_wasm: upload(&env, "saturn_project_token.wasm"),
            vault_wasm: upload(&env, "saturn_staking_vault.wasm"),
            curve_wasm: market.clone(), pool_wasm: upload(&env, "saturn_market_test_fixture.wasm"),
        },
    };
    let request = LaunchRequest {
        salt: BytesN::from_array(&env, &[33; 32]), max_creation_fee: 100 * UNIT,
        expected_target: 30_000 * UNIT, lock_metadata: true,
    };
    let id = env.register(LaunchFactory, (&config,));
    let failed = LaunchFactoryClient::new(&env, &id).create_launch(&creator, &metadata(&env), &request);
    let curve = MarketClient::new(&env, &failed.addresses.curve);
    let currency = TokenClient::new(&env, &xlm);
    let tokens = TokenClient::new(&env, &failed.addresses.token);
    let quote = curve.quote_curve(&true, &HALF);
    assert_eq!(quote.xlm, 12_500 * UNIT);
    assert!(curve.try_trade_curve(&trader, &true, &HALF, &(quote.input - 1), &quote.output, &100).is_err());
    curve.trade_curve(&trader, &true, &HALF, &quote.input, &quote.output, &100);
    let wallet_before = currency.balance(&trader);
    let token_before = tokens.balance(&trader);
    let treasury_before = currency.balance(&treasury);
    let close = curve.quote_curve(&true, &HALF);
    assert!(curve.try_trade_curve(&trader, &true, &HALF, &close.input, &close.output, &100).is_err());
    assert_eq!(currency.balance(&trader), wallet_before);
    assert_eq!(tokens.balance(&trader), token_before);
    assert_eq!(currency.balance(&treasury), treasury_before);
    assert_eq!(currency.balance(&failed.addresses.curve), 12_500 * UNIT);
    assert_eq!(currency.balance(&failed.addresses.pool), 0);
    assert_eq!(tokens.balance(&failed.addresses.pool), 0);
    let exit = curve.quote_curve(&false, &(100 * UNIT));
    curve.trade_curve(&trader, &false, &(100 * UNIT), &exit.input, &exit.output, &100);
    assert_eq!(currency.balance(&failed.addresses.curve), 12_500 * UNIT - exit.xlm);

    config.template.pool_wasm = market;
    let id = env.register(wasm("saturn_launch_factory.wasm").as_slice(), (&config,));
    let factory = LaunchFactoryClient::new(&env, &id);
    let fields = metadata(&env);
    let predicted = factory.predict_launch(&creator, &request.salt);
    let roles = LaunchRoles {
        curve: predicted.curve.clone(),
        amm_inventory: predicted.curve.clone(),
        reward_controller: predicted.vault.clone(),
        reward_vault: predicted.vault.clone(),
    };
    let factory_auth = soroban_sdk::testutils::MockAuth {
        address: &creator,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &id,
            fn_name: "create_launch",
            args: (&creator, &fields, &request).into_val(&env),
            sub_invokes: &[
                soroban_sdk::testutils::MockAuthInvoke {
                    contract: &xlm,
                    fn_name: "transfer",
                    args: (&creator, &treasury, 100 * UNIT).into_val(&env),
                    sub_invokes: &[],
                },
                soroban_sdk::testutils::MockAuthInvoke {
                    contract: &predicted.token,
                    fn_name: "__constructor",
                    args: (&creator, &fields, request.lock_metadata, &roles).into_val(&env),
                    sub_invokes: &[],
                },
            ],
        },
    };
    let launched = factory.mock_auths(&[factory_auth]).create_launch(&creator, &fields, &request);
    std::println!("Factory launch host resources: {:?}", env.cost_estimate().resources());
    let curve = MarketClient::new(&env, &launched.addresses.curve);
    let pool = MarketClient::new(&env, &launched.addresses.pool);
    let tokens = TokenClient::new(&env, &launched.addresses.token);
    let vault = StakeClient::new(&env, &launched.addresses.vault);
    assert_eq!(pool.reserves(), None);
    assert_eq!(pool.share_supply(), None);
    assert!(pool.try_quote_deposit(&UNIT, &UNIT).is_err());
    assert!(pool.try_quote_swap(&true, &UNIT).is_err());
    env.set_auths(&[]);
    assert!(pool.try_seed(&(29_850 * UNIT), &SEED).is_err());
    env.mock_all_auths_allowing_non_root_auth();
    let buy = curve.quote_curve(&true, &(1_000_000 * UNIT));
    curve.trade_curve(&trader, &true, &buy.tokens, &buy.input, &buy.output, &100);
    let redeem = curve.quote_curve(&false, &(100_000 * UNIT));
    tokens.transfer(&trader, &creator, &redeem.input);
    curve.trade_curve(&creator, &false, &redeem.tokens, &redeem.input, &redeem.output, &100);
    assert_eq!(currency.balance(&launched.addresses.curve), buy.xlm - redeem.xlm);

    let stake = 1_000 * UNIT;
    vault.queue_stake(&trader, &stake);
    assert_eq!(vault.principal_liability(), stake);
    let close_size = 2 * HALF - buy.tokens + redeem.tokens - 1_990_000;
    let close = curve.quote_curve(&true, &close_size);
    let graduation: Val = close.graduation.clone().unwrap().into_val(&env);
    let amounts = Graduation::try_from_val(&env, &graduation).unwrap();
    assert_eq!(amounts.dust, 1_990_000);
    assert_eq!(amounts.gross_xlm, 30_000 * UNIT - 199);
    assert_eq!(amounts.fee, 150 * UNIT - 1);
    let treasury_before = currency.balance(&treasury);
    let auth = soroban_sdk::testutils::MockAuth {
        address: &trader,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &launched.addresses.curve,
            fn_name: "trade_curve",
            args: (&trader, true, close_size, close.input, close.output, 100u64).into_val(&env),
            sub_invokes: &[soroban_sdk::testutils::MockAuthInvoke {
                contract: &xlm,
                fn_name: "transfer",
                args: (&trader, &launched.addresses.curve, close.input).into_val(&env),
                sub_invokes: &[],
            }],
        },
    };
    curve.mock_auths(&[auth]).trade_curve(&trader, &true, &close_size, &close.input, &close.output, &100);
    std::println!("Atomic graduation host resources: {:?}", env.cost_estimate().resources());
    assert_eq!(currency.balance(&treasury) - treasury_before, amounts.fee);
    assert_eq!(vault.principal_liability(), stake);
    assert_eq!(currency.balance(&launched.addresses.curve), 0);
    assert_eq!(tokens.balance(&launched.addresses.curve), 0);
    assert!(curve.try_quote_curve(&false, &UNIT).is_err());
    let burn_day: i128 = env.invoke_contract(&launched.addresses.token,
        &soroban_sdk::Symbol::new(&env, "burned_on"), (0u64,).into_val(&env));
    assert_eq!(burn_day, buy.token_fee + redeem.token_fee + close.token_fee + amounts.dust);
    let seeded = pool.reserves().unwrap();
    assert_eq!(seeded, Reserves { xlm: amounts.pool_xlm, tokens: SEED });
    env.mock_all_auths_allowing_non_root_auth();
    assert!(pool.try_seed(&seeded.xlm, &seeded.tokens).is_err());
    let genesis = pool.share_supply().unwrap();
    assert_eq!(genesis.total, genesis.locked);
    assert_eq!(genesis.locked, ((seeded.xlm * seeded.tokens) as u128).isqrt() as i128);
    assert_eq!(pool.lp_balance(&creator), 0);
    assert!(pool.try_quote_withdraw(&genesis.locked).is_err());
    let budget_xlm = 100 * UNIT;
    let budget_token = 2_000_000 * UNIT;
    let liquidity = pool.quote_deposit(&budget_xlm, &budget_token);
    assert!(liquidity.tokens < budget_token);
    let creator_xlm = currency.balance(&creator);
    assert!(pool.try_deposit(&creator, &budget_xlm, &budget_token, &liquidity.shares, &100).is_err());
    assert_eq!(currency.balance(&creator), creator_xlm);
    assert_eq!(pool.reserves(), Some(seeded.clone()));
    assert!(pool.try_deposit(&trader, &budget_xlm, &budget_token, &(liquidity.shares + 1), &100).is_err());
    assert_eq!(pool.deposit(&trader, &budget_xlm, &budget_token, &liquidity.shares, &100), liquidity);
    assert_eq!(pool.lp_balance(&trader), liquidity.shares);
    let before_swaps = pool.reserves().unwrap();
    let buy = pool.quote_swap(&true, &(100 * UNIT));
    pool.swap(&trader, &true, &buy.xlm, &buy.input, &buy.output, &100);
    let after_buy = pool.reserves().unwrap();
    let sell = pool.quote_swap(&false, &(100_000 * UNIT));
    pool.swap(&trader, &false, &sell.tokens, &sell.input, &sell.output, &100);
    let after_sell = pool.reserves().unwrap();
    assert!(after_buy.xlm * after_buy.tokens >= before_swaps.xlm * before_swaps.tokens);
    assert!(after_sell.xlm * after_sell.tokens >= after_buy.xlm * after_buy.tokens);
    assert_eq!(currency.balance(&launched.addresses.pool), after_sell.xlm);
    assert_eq!(tokens.balance(&launched.addresses.pool), after_sell.tokens);
    let exit = pool.quote_withdraw(&liquidity.shares);
    assert!(pool.try_withdraw(&creator, &exit.shares, &exit.xlm, &exit.tokens, &100).is_err());
    assert!(pool.try_withdraw(&trader, &exit.shares, &(exit.xlm + 1), &exit.tokens, &100).is_err());
    let xlm_before_exit = currency.balance(&trader);
    let tokens_before_exit = tokens.balance(&trader);
    assert_eq!(pool.withdraw(&trader, &exit.shares, &exit.xlm, &exit.tokens, &100), exit);
    assert_eq!(currency.balance(&trader), xlm_before_exit + exit.xlm);
    assert_eq!(tokens.balance(&trader), tokens_before_exit + exit.tokens);
    assert_eq!(pool.lp_balance(&trader), 0);
    assert_eq!(pool.share_supply(), Some(genesis));
    assert_eq!(pool.reserves(), Some(Reserves { xlm: after_sell.xlm - exit.xlm, tokens: after_sell.tokens - exit.tokens }));

    env.ledger().set_timestamp(2 * 86_400);
    vault.roll(&2);
    let empty = vault.settle_rewards();
    assert_eq!((empty.day, empty.power, empty.minted), (0, 0, 0));
    let eligible = vault.settle_rewards();
    assert_eq!((eligible.day, eligible.power), (1, stake * 180));
    assert!(eligible.minted > eligible.recycled && eligible.recycled > 0);
    let before_claim = tokens.balance(&trader);
    assert_eq!(vault.claim(&trader, &16).paid, eligible.minted);
    assert_eq!(tokens.balance(&trader) - before_claim, eligible.minted);
    assert_eq!(vault.principal_liability(), stake);
}
