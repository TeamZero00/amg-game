use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw20::Expiration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub denom: String,
    pub fee_late: u8,
    // pub oracle_address: String, //솔루션 나오면 업데이트
    pub price: String,
    pub minimum_amount: u64,
    pub bank_contract_address: String,
    pub betting_deadline_height: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Betting { position: String, duration: u64 },
    Setting { price: Uint128, lock: Option<bool> },
    SetFeeLate { fee_late: u8 },
    SetMinimumAmount { amount: u64 },
    SetBankContract { address: String },
    // SetBettingDeadline { deadline_height: u64 },
    AddAdmin { address: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetState {},
    GetBalance { address: String },
    GetRoundPrice { height: u64 },
    GetLatestPrice {},
    GetHeightBettingList { target_height: u64 },
    GetisLock {}, // GetLPContract {},
    GetRecentBettingList { target_height: u64 },
}

#[cw_serde]
pub struct AllowanceAndTotalSupplyResponse {
    pub allowance: Uint128,
    pub expires: Expiration,
    pub total_supply: Uint128,
}
// We define a custom struct for each query response

#[cw_serde]
pub enum AMGBankMsg {
    Deposit {},
    Withdraw {},
    BorrowBalance { amount: Uint128 },
    PayBack {},
}
