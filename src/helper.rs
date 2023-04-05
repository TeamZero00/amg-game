use crate::error::ContractError;
use crate::state::{Betting, Position, State, BALANCE};

use cosmwasm_std::{Decimal, DepsMut, MessageInfo, StdResult, Uint128};

pub fn check_admin(info: &MessageInfo, state: &State) -> Result<(), ContractError> {
    match state.admin == info.sender {
        true => Ok(()),
        false => Err(ContractError::OnlyOwner {}),
    }
}

pub fn check_denom(info: &MessageInfo, state: &State) -> Result<(), ContractError> {
    //token check
    match info.funds.len() {
        0 => Err(ContractError::MustSendCoin {}),
        1 => Ok(()),
        _ => Err(ContractError::InvalidOneTypeCoin {}),
    }?;

    let coin = &info.funds[0];
    //denom_check
    match coin.denom == state.denom {
        true => Ok(()),
        false => Err(ContractError::InvalidDenom {}),
    }?;

    match coin.amount >= state.minimum_amount {
        true => Ok(()),
        false => Err(ContractError::InvalidMinimumAmount {}),
    }
}

pub fn check_duration(duration: u64) -> Result<(), ContractError> {
    match duration {
        duration if duration >= 20 && duration <= 120 => Ok(()),
        _ => Err(ContractError::InvalidDuration {}),
    }
}

pub fn betting_calculate(
    options: &Vec<Betting>,
    deps: &mut DepsMut,
    now_price: Decimal,
) -> StdResult<Uint128> {
    let mut return_balance = Uint128::new(0);

    for option in options {
        let base_price = option.base_price;
        let win_position = match base_price.cmp(&now_price) {
            std::cmp::Ordering::Less => Position::Long,
            std::cmp::Ordering::Equal => Position::Eqaul,
            std::cmp::Ordering::Greater => Position::Short,
        };
        let prize_amount = option
            .amount
            .checked_mul(Uint128::new(2))
            .unwrap_or_else(|_| Uint128::MAX);

        if win_position != option.position {
            return_balance += prize_amount;
            continue;
        }

        BALANCE.update(deps.storage, &option.addr, |exsists| -> StdResult<_> {
            match exsists {
                Some(mut balance) => {
                    balance += prize_amount;
                    Ok(balance)
                }
                None => Ok(prize_amount),
            }
        })?;
    }

    Ok(return_balance)
}
