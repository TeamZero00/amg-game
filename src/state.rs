use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, CanonicalAddr, Decimal, StdResult, Storage, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

use crate::ContractError;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub admin: Vec<Addr>,
    pub fee_late: u8,
    pub denom: String,
    pub minimum_amount: Uint128,
    pub bank_contract: Addr,
    pub latest_price: Uint128,
    pub lock: bool,
}

pub fn save_state(storage: &mut dyn Storage, state: &State) -> StdResult<()> {
    STATE.save(storage, state)
}

//State 스토리지 읽어오는 함수
pub fn load_state(storage: &dyn Storage) -> StdResult<State> {
    STATE.load(storage)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Position {
    Long,
    Short,
    Eqaul,
}

impl Position {
    pub fn new(position: &str) -> Result<Self, ContractError> {
        match position {
            "long" => Ok(Position::Long),
            "short" => Ok(Position::Short),
            "equal" => Ok(Position::Eqaul),
            _ => Err(ContractError::InvalidPosition {}),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Betting {
    pub address: Addr,
    pub start_height: u64,
    pub target_height: u64,
    pub position: Position,
    pub base_price: Uint128,
    pub amount: Uint128,
    pub win_amount: Uint128,
}
impl Betting {
    pub fn new(
        address: Addr,
        position: Position,
        amount: Uint128,
        win_amount: Uint128,
        base_price: Uint128,
        start_height: u64,
        target_height: u64,
    ) -> Self {
        Betting {
            address,
            position,
            amount,
            base_price,
            start_height,
            target_height,
            win_amount,
        }
    }
}

pub const STATE: Item<State> = Item::new("state");
// key - target_height
pub const BETTINGS: Map<u64, Vec<Betting>> = Map::new("bettings");

pub const BALANCE: Map<&Addr, Uint128> = Map::new("balance");
pub const PRICES: Map<u64, Uint128> = Map::new("prices");
