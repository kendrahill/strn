use soroban_sdk::{contractevent, Address};

#[contractevent]
pub struct Approve {
    #[topic]
    pub from: Address,
    #[topic]
    pub spender: Address,
    pub amount: i128,
    pub live_until_ledger: u32,
}

#[contractevent]
pub struct Transfer {
    #[topic]
    pub from: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
    pub to_muxed_id: Option<u64>,
}

#[contractevent(data_format = "single-value")]
pub struct Burn {
    #[topic]
    pub from: Address,
    pub amount: i128,
}

#[contractevent(data_format = "single-value")]
pub struct Mint {
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contractevent]
pub struct CapacityRecycled {
    #[topic]
    pub day: u64,
    pub amount: i128,
}

#[contractevent]
pub struct RewardsReleased {
    #[topic]
    pub day: u64,
    pub eligible: bool,
    pub initial: i128,
    pub recycled: i128,
}
