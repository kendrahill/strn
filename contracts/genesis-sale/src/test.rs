extern crate std;

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, token::StellarAssetClient};

#[test]
fn purchases_pay_and_mint_atomically_through_sellout() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let treasury = Address::generate(&env);
    let recipient = Address::generate(&env);
    let spender = Address::generate(&env);
    let poor = Address::generate(&env);
    let xlm = env.register_stellar_asset_contract_v2(admin).address();
    let price = 1_000i128;
    let config = SaleConfig { xlm: xlm.clone(), treasury: treasury.clone(), price };
    let metadata = Vec::from_iter(&env, (1..=100).map(|id| {
        String::from_str(&env, &std::format!("ar://4XthyDsGTztwFE6JvAObf0QAZsb1tGUDbPEDr2vXquQ/{id}.json"))
    }));
    let id = env.register(GenesisSale, (&config, &metadata));
    let sale = GenesisSaleClient::new(&env, &id);
    assert_eq!(sale.total_supply(), 0);
    assert_eq!(sale.max_supply(), 100);
    assert!(sale.try_buy(&buyer, &1, &price, &100).is_err());
    env.mock_all_auths();
    StellarAssetClient::new(&env, &xlm).mint(&buyer, &(101 * price));
    let payment = TokenClient::new(&env, &xlm);
    env.ledger().set_timestamp(10);
    assert_eq!(sale.try_buy(&buyer, &1, &(price - 1), &100), Err(Ok(SaleError::AboveLimit)));
    assert_eq!(sale.try_buy(&buyer, &1, &price, &9), Err(Ok(SaleError::Expired)));
    assert!(sale.try_buy(&poor, &1, &price, &100).is_err());
    assert_eq!(payment.balance(&treasury), 0);
    assert_eq!(sale.total_supply(), 0);
    assert_eq!(sale.buy(&buyer, &1, &price, &100).metadata_uri, metadata.get_unchecked(0));
    assert_eq!(sale.try_buy(&buyer, &1, &price, &100), Err(Ok(SaleError::QuoteChanged)));
    assert_eq!(payment.balance(&treasury), price);
    sale.approve(&buyer, &spender, &1, &100);
    sale.transfer_from(&spender, &buyer, &recipient, &1);
    assert_eq!(sale.get_approved(&1), None);
    assert_eq!(sale.owner_of(&1), recipient);
    for token in 2..=100 {
        env.cost_estimate().budget().reset_default();
        assert_eq!(sale.buy(&buyer, &token, &price, &100).token_id, token);
        assert_eq!(sale.token_uri(&token), metadata.get_unchecked(token - 1));
    }
    assert_eq!(sale.try_buy(&buyer, &101, &price, &100), Err(Ok(SaleError::SoldOut)));
    assert_eq!(sale.total_supply(), 100);
    assert_eq!(sale.balance(&buyer), 99);
    assert_eq!(payment.balance(&treasury), 100 * price);
    assert_eq!(payment.balance(&buyer), price);
}
