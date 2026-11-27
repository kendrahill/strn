#![no_std]

use saturn_token_metadata::is_arweave_uri;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error,
    token::TokenClient, Address, Env, Map, String, Vec,
};

const MAX_SUPPLY: u32 = 100;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleConfig {
    pub xlm: Address,
    pub treasury: Address,
    pub price: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Purchase {
    pub token_id: u32,
    pub price: i128,
    pub metadata_uri: String,
}

#[contracttype]
#[derive(Clone)]
struct Approval {
    address: Address,
    expires: u32,
}

#[contracttype]
enum Key {
    Config,
    Metadata,
    Minted,
    Owner(u32),
    Balance(Address),
    Approval(u32),
    Operator(Address, Address),
}

#[contractevent]
struct Mint {
    #[topic]
    to: Address,
    token_id: u32,
}

#[contractevent]
struct Purchased {
    #[topic]
    buyer: Address,
    #[topic]
    token_id: u32,
    price: i128,
}

#[contractevent(data_format = "single-value")]
struct Transfer {
    #[topic]
    from: Address,
    #[topic]
    to: Address,
    token_id: u32,
}

#[contractevent(data_format = "vec")]
struct Approve {
    #[topic]
    from: Address,
    #[topic]
    token_id: u32,
    to: Address,
    live_until_ledger: u32,
}

#[contractevent(data_format = "single-value")]
struct ApproveForAll {
    #[topic]
    owner: Address,
    #[topic]
    operator: Address,
    live_until_ledger: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SaleError {
    InvalidConfig = 1,
    InvalidMetadata = 2,
    SoldOut = 3,
    AboveLimit = 4,
    Expired = 5,
    InvalidBuyer = 6,
    PaymentMismatch = 7,
    UnknownToken = 8,
    Unauthorized = 9,
    QuoteChanged = 10,
}

fn store_balance(env: &Env, address: &Address, balance: u32) {
    let key = Key::Balance(address.clone());
    let storage = env.storage().persistent();
    if balance == 0 {
        storage.remove(&key);
    } else {
        storage.set(&key, &balance);
    }
}

fn move_nft(env: &Env, from: Address, to: Address, token_id: u32) {
    let storage = env.storage().persistent();
    storage.remove(&Key::Approval(token_id));
    if from != to {
        store_balance(env, &from, GenesisSale::balance(env.clone(), from.clone()) - 1);
        store_balance(env, &to, GenesisSale::balance(env.clone(), to.clone()) + 1);
        storage.set(&Key::Owner(token_id), &to);
    }
    Transfer { from, to, token_id }.publish(env);
}

#[contract]
pub struct GenesisSale;

#[contractimpl]
impl GenesisSale {
    pub fn __constructor(env: Env, config: SaleConfig, metadata: Vec<String>) -> Result<(), SaleError> {
        if config.price <= 0 || config.price > i128::from(i64::MAX)
            || config.treasury == env.current_contract_address()
        {
            return Err(SaleError::InvalidConfig);
        }
        if metadata.len() != MAX_SUPPLY {
            return Err(SaleError::InvalidMetadata);
        }
        let mut seen = Map::<String, bool>::new(&env);
        for uri in metadata.iter() {
            if !is_arweave_uri(&uri) || seen.contains_key(uri.clone()) {
                return Err(SaleError::InvalidMetadata);
            }
            seen.set(uri, true);
        }
        let storage = env.storage().instance();
        storage.set(&Key::Config, &config);
        storage.set(&Key::Metadata, &metadata);
        storage.set(&Key::Minted, &0u32);
        Ok(())
    }

    pub fn name(env: Env) -> String { String::from_str(&env, "Saturn Genesis") }
    pub fn symbol(env: Env) -> String { String::from_str(&env, "SATGEN") }
    pub fn max_supply() -> u32 { MAX_SUPPLY }
    pub fn total_supply(env: Env) -> u32 { env.storage().instance().get(&Key::Minted).unwrap() }
    pub fn config(env: Env) -> SaleConfig { env.storage().instance().get(&Key::Config).unwrap() }

    pub fn quote(env: Env) -> Result<Purchase, SaleError> {
        let token_id = Self::total_supply(env.clone()) + 1;
        if token_id > MAX_SUPPLY {
            return Err(SaleError::SoldOut);
        }
        let metadata: Vec<String> = env.storage().instance().get(&Key::Metadata).unwrap();
        Ok(Purchase {
            token_id, price: Self::config(env).price, metadata_uri: metadata.get_unchecked(token_id - 1),
        })
    }

    pub fn buy(env: Env, buyer: Address, expected_token_id: u32, max_price: i128, deadline: u64) -> Result<Purchase, SaleError> {
        buyer.require_auth();
        if deadline < env.ledger().timestamp() {
            return Err(SaleError::Expired);
        }
        let quote = Self::quote(env.clone())?;
        if quote.token_id != expected_token_id {
            return Err(SaleError::QuoteChanged);
        }
        if quote.price > max_price {
            return Err(SaleError::AboveLimit);
        }
        let config = Self::config(env.clone());
        if buyer == config.treasury {
            return Err(SaleError::InvalidBuyer);
        }
        let xlm = TokenClient::new(&env, &config.xlm);
        let previous = xlm.balance(&config.treasury);
        xlm.transfer(&buyer, &config.treasury, &quote.price);
        if xlm.balance(&config.treasury) - previous != quote.price {
            return Err(SaleError::PaymentMismatch);
        }
        env.storage().persistent().set(&Key::Owner(quote.token_id), &buyer);
        store_balance(&env, &buyer, Self::balance(env.clone(), buyer.clone()) + 1);
        env.storage().instance().set(&Key::Minted, &quote.token_id);
        Mint { to: buyer.clone(), token_id: quote.token_id }.publish(&env);
        Purchased { buyer, token_id: quote.token_id, price: quote.price }.publish(&env);
        Ok(quote)
    }

    pub fn owner_of(env: Env, token_id: u32) -> Address {
        env.storage().persistent().get(&Key::Owner(token_id))
            .unwrap_or_else(|| panic_with_error!(&env, SaleError::UnknownToken))
    }

    pub fn token_uri(env: Env, token_id: u32) -> String {
        if token_id == 0 || token_id > Self::total_supply(env.clone()) {
            panic_with_error!(&env, SaleError::UnknownToken);
        }
        let metadata: Vec<String> = env.storage().instance().get(&Key::Metadata).unwrap();
        metadata.get_unchecked(token_id - 1)
    }

    pub fn balance(env: Env, account: Address) -> u32 {
        env.storage().persistent().get(&Key::Balance(account)).unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: u32) {
        from.require_auth();
        if Self::owner_of(env.clone(), token_id) != from {
            panic_with_error!(&env, SaleError::Unauthorized);
        }
        move_nft(&env, from, to, token_id);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, token_id: u32) {
        spender.require_auth();
        if Self::owner_of(env.clone(), token_id) != from || (
            spender != from && Self::get_approved(env.clone(), token_id).as_ref() != Some(&spender)
            && !Self::is_approved_for_all(env.clone(), from.clone(), spender)
        ) {
            panic_with_error!(&env, SaleError::Unauthorized);
        }
        move_nft(&env, from, to, token_id);
    }

    pub fn get_approved(env: Env, token_id: u32) -> Option<Address> {
        Self::owner_of(env.clone(), token_id);
        env.storage().persistent().get::<_, Approval>(&Key::Approval(token_id))
            .filter(|value| value.expires >= env.ledger().sequence()).map(|value| value.address)
    }

    pub fn approve(env: Env, approver: Address, approved: Address, token_id: u32, live_until_ledger: u32) {
        approver.require_auth();
        let owner = Self::owner_of(env.clone(), token_id);
        if approver != owner && !Self::is_approved_for_all(env.clone(), owner.clone(), approver) {
            panic_with_error!(&env, SaleError::Unauthorized);
        }
        let storage = env.storage().persistent();
        let key = Key::Approval(token_id);
        if approved == owner || live_until_ledger == 0 {
            storage.remove(&key);
        } else {
            if live_until_ledger < env.ledger().sequence() {
                panic_with_error!(&env, SaleError::Expired);
            }
            storage.set(&key, &Approval { address: approved.clone(), expires: live_until_ledger });
        }
        Approve { from: owner, token_id, to: approved, live_until_ledger }.publish(&env);
    }

    pub fn is_approved_for_all(env: Env, owner: Address, operator: Address) -> bool {
        env.storage().persistent().get::<_, u32>(&Key::Operator(owner, operator))
            .is_some_and(|expires| expires >= env.ledger().sequence())
    }

    pub fn approve_for_all(env: Env, owner: Address, operator: Address, live_until_ledger: u32) {
        owner.require_auth();
        let key = Key::Operator(owner.clone(), operator.clone());
        let storage = env.storage().persistent();
        if live_until_ledger == 0 {
            storage.remove(&key);
        } else {
            if live_until_ledger < env.ledger().sequence() {
                panic_with_error!(&env, SaleError::Expired);
            }
            storage.set(&key, &live_until_ledger);
        }
        ApproveForAll { owner, operator, live_until_ledger }.publish(&env);
    }
}

#[cfg(test)]
mod test;
