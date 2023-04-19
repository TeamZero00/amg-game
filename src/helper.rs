use crate::error::ContractError;
use crate::state::State;

use cosmwasm_std::MessageInfo;

pub fn check_admin(info: &MessageInfo, state: &State) -> Result<(), ContractError> {
    // match state.admin == info.sender {
    //     true => Ok(()),
    //     false => Err(ContractError::OnlyOwner {}),
    // }
    match state.admin.contains(&info.sender) {
        true => Ok(()),
        false => Err(ContractError::OnlyOwner {}),
    }
}
pub fn check_lock(state: &State) -> Result<(), ContractError> {
    match state.lock {
        true => Err(ContractError::Lock {}),
        false => Ok(()),
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

// pub fn check_dead_line(state: &State, env: &Env, duration: u64) -> Result<(), ContractError> {
//     let target_height = env.block.height + duration;
//     match target_height.cmp(&state.betting_deadline_height) {
//         Less | Equal => Ok(()),
//         Greater => Err(ContractError::OverDeadline {}),
//     }
// }

//block_height + 1 = 6s
pub fn check_duration(duration: u64) -> Result<(), ContractError> {
    match duration {
        30 | 50 => Ok(()),
        _ => Err(ContractError::InvalidDuration {}),
    }
}

// pub fn betting_calculate(
//     bettings: &Vec<Betting>,
//     deps: &mut DepsMut,
//     now_price: Decimal,
// ) -> StdResult<Uint128> {
//     let mut return_balance = Uint128::new(0);

//     for betting in bettings {
//         let base_price = betting.base_price;
//         let win_position = match base_price.cmp(&now_price) {
//             std::cmp::Ordering::Less => Position::Long,
//             std::cmp::Ordering::Equal => Position::Eqaul,
//             std::cmp::Ordering::Greater => Position::Short,
//         };
//         let prize_amount = betting
//             .amount
//             .checked_mul(Uint128::new(2))
//             .unwrap_or_else(|_| Uint128::MAX);

//         if win_position != betting.position {
//             return_balance += prize_amount;
//             continue;
//         }

//         BALANCE.update(deps.storage, &betting.address, |exsists| -> StdResult<_> {
//             match exsists {
//                 Some(mut balance) => {
//                     balance += prize_amount;
//                     Ok(balance)
//                 }
//                 None => Ok(prize_amount),
//             }
//         })?;
//     }

//     Ok(return_balance)
// }
