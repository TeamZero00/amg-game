use crate::error::ContractError;
use crate::helper::{check_admin, check_denom, check_duration, check_lock};
use crate::msg::{AMGBankMsg, ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::state::{load_state, save_state, Betting, Position, State, BALANCE, BETTINGS, PRICES};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, Uint128, WasmMsg,
};
use std::cmp::Ordering::*;

use cw2::set_contract_version;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:fx-game";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let bank_contract = deps.api.addr_validate(&msg.bank_contract_address)?;
    let state = State {
        admin: vec![info.sender.clone()],
        denom: msg.denom.clone(),
        fee_late: msg.fee_late,
        minimum_amount: Uint128::new(msg.minimum_amount as u128),
        bank_contract,
        latest_price: Uint128::new(0),
        lock: false,
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    save_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender)
        .add_attribute("denom", msg.denom)
        .add_attribute("fee_late", msg.fee_late.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Betting { position, duration } => betting(deps, env, info, position, duration),
        ExecuteMsg::Setting { price, lock } => setting(deps, env, info, price, lock),

        ExecuteMsg::SetFeeLate { fee_late } => set_fee_late(deps, env, info, fee_late),
        ExecuteMsg::SetMinimumAmount { amount } => set_minimum_amount(deps, env, info, amount),
        ExecuteMsg::SetBankContract { address } => set_bank_contract(deps, info, address),
        ExecuteMsg::AddAdmin { address } => add_admin(deps, info, address),
    }
}

fn betting(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    position: String,
    duration: u64,
) -> Result<Response, ContractError> {
    let state = load_state(deps.storage)?;
    check_lock(&state)?;
    check_denom(&info, &state)?;
    check_duration(duration)?;

    let now_height = env.block.height;
    let denom = &info.funds[0];

    let base_price = match PRICES.load(deps.storage, now_height) {
        Ok(price) => price,
        Err(_) => state.latest_price.clone(),
    };

    let target_height = now_height + duration;
    let betting_amount = denom.amount;

    //3/100 = 0.03
    let fee_late = Decimal::from_ratio(state.fee_late, Uint128::new(100));
    //borrow amount = 0.97 * betting_amount
    let borrow_amount = (Decimal::one().checked_sub(fee_late)).unwrap() * betting_amount;
    let win_amount = betting_amount + borrow_amount;
    //option 업데이트
    {
        let position = Position::new(position.as_str())?;
        let betting = Betting::new(
            info.sender.clone(),
            position,
            betting_amount,
            win_amount,
            base_price,
            now_height,
            target_height,
        );

        BETTINGS.update(deps.storage, target_height, |exsists| -> StdResult<_> {
            match exsists {
                Some(mut bettings) => {
                    bettings.push(betting);
                    Ok(bettings)
                }
                None => Ok(vec![betting]),
            }
        })?;
    }

    // save_state(deps.storage, &state)?;

    let msg = AMGBankMsg::BorrowBalance {
        amount: borrow_amount,
    };

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state.bank_contract.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        }))
        .add_attributes(vec![
            ("method", "betting".to_string()),
            ("position", position),
            ("account", info.sender.to_string()),
            ("betting_amount", betting_amount.to_string()),
            ("start_height", now_height.to_string()),
            ("target_height", target_height.to_string()),
            ("price", base_price.to_string()),
        ]))
}

/*price = 1.00001 => 100001 */
fn setting(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    price: Uint128,
    lock: Option<bool>,
) -> Result<Response, ContractError> {
    //
    let mut state = load_state(deps.storage)?;

    check_admin(&info, &state)?;
    match lock {
        Some(lock) => state.lock = lock,
        None => {}
    }
    let now_height = env.block.height;

    // Save the new price
    /*
    If you set the price at the current block height,
    there is a possibility that users' now_price will be different,
    so we add + 1 to allow them to bet at the same price.
     */
    //next round setting
    PRICES.save(deps.storage, now_height + 1, &price)?;

    let mut return_balance = Uint128::new(0);
    let mut bank_msgs = vec![];

    let mut attrs = vec![("action".to_string(), "setting".to_string())];
    let round_price = PRICES.load(deps.storage, env.block.height);
    match round_price {
        Ok(round_price) => {
            attrs.push(("round_price".to_string(), round_price.to_string()));
            let bettings = BETTINGS
                .load(deps.storage, now_height)
                .unwrap_or_else(|_| vec![]);

            if !bettings.is_empty() {
                for betting in bettings {
                    let base_price = betting.base_price;
                    let win_position = match base_price.cmp(&round_price) {
                        Less => Position::Long,
                        Equal => Position::Eqaul,
                        Greater => Position::Short,
                    };

                    if win_position != betting.position {
                        return_balance += betting.win_amount;
                        continue;
                    }

                    let bank_msg = CosmosMsg::Bank(BankMsg::Send {
                        to_address: betting.address.to_string(),
                        amount: vec![coin(betting.win_amount.u128(), "uconst")],
                    });

                    bank_msgs.push(bank_msg);

                    attrs.push((betting.address.to_string(), betting.win_amount.to_string()))
                }

                //delete
                BETTINGS.remove(deps.storage, now_height);
            }
        }

        Err(_) => {
            //how many
            let mut before_bettings = vec![];
            for i in 1..=5 {
                let before_betting = BETTINGS
                    .load(deps.storage, env.block.height - i)
                    .unwrap_or_else(|_| vec![]);
                before_bettings.push(before_betting)
            }

            match before_bettings.is_empty() {
                true => {}
                false => {
                    let before_bettings = before_bettings
                        .into_iter()
                        .flatten()
                        .collect::<Vec<Betting>>();
                    for betting in before_bettings {
                        //betting amount 는 수수료 포함 금액

                        let bank_msg = CosmosMsg::Bank(BankMsg::Send {
                            to_address: betting.address.to_string(),
                            amount: vec![coin(betting.amount.u128(), "uconst")],
                        });

                        let fee_late = Decimal::from_ratio(state.fee_late, Uint128::new(100));
                        let borrowed_amount =
                            (Decimal::one().checked_sub(fee_late)).unwrap() * betting.amount;
                        return_balance += borrowed_amount;
                        bank_msgs.push(bank_msg)
                    }
                    BETTINGS.remove(deps.storage, env.block.height - 1)
                }
            };
        }
    };

    state.latest_price = price;

    save_state(deps.storage, &state)?;
    let response = match return_balance.is_zero() {
        true => Response::new()
            .add_messages(bank_msgs)
            .add_attributes(attrs),

        false => Response::new()
            .add_messages(bank_msgs)
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: state.bank_contract.to_string(),
                msg: to_binary(&AMGBankMsg::PayBack {})?,
                funds: vec![coin(return_balance.u128(), "uconst")],
            }))
            .add_attributes(attrs),
    };
    Ok(response)
}
fn add_admin(deps: DepsMut, info: MessageInfo, address: String) -> Result<Response, ContractError> {
    let mut state = load_state(deps.storage)?;
    check_admin(&info, &state)?;
    let new_admin = deps.api.addr_validate(address.as_str())?;
    state.admin.push(new_admin);
    save_state(deps.storage, &state)?;

    Ok(Response::new())
}

fn set_fee_late(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    fee_late: u8,
) -> Result<Response, ContractError> {
    let mut state = load_state(deps.storage)?;
    check_admin(&info, &state)?;
    state.fee_late = fee_late;
    save_state(deps.storage, &state)?;
    Ok(Response::new()
        .add_attribute("method", "set_fee_late")
        .add_attribute("fee_late", fee_late.to_string()))
}

fn set_minimum_amount(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: u64,
) -> Result<Response, ContractError> {
    let mut state = load_state(deps.storage)?;
    check_admin(&info, &state)?;
    state.minimum_amount = amount.into();
    save_state(deps.storage, &state)?;
    Ok(Response::new())
}

fn set_bank_contract(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    let mut state = load_state(deps.storage)?;
    check_admin(&info, &state)?;
    let pool_contract = deps.api.addr_validate(address.as_str())?;
    state.bank_contract = pool_contract;
    save_state(deps.storage, &state)?;
    Ok(Response::new().add_attribute("bank_contract", address))
}

// ######## TODO!!! Oracle version Setting

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetBalance { address } => to_binary(&query_get_account_balance(deps, address)?),
        QueryMsg::GetRoundPrice { height } => to_binary(&query_get_round_price(deps, height)?),
        QueryMsg::GetLatestPrice {} => to_binary(&query_get_latest_price(deps)?),
        QueryMsg::GetHeightBettingList { target_height } => {
            to_binary(&query_get_height_betting_list(deps, target_height)?)
        }
        QueryMsg::GetisLock {} => to_binary(&query_state_lock(deps)?),
        QueryMsg::GetRecentBettingList { target_height } => {
            to_binary(&query_get_recent_betting_list(deps, target_height)?)
        }
    }
}

fn query_state(deps: Deps) -> StdResult<State> {
    let state = load_state(deps.storage)?;
    Ok(state)
}

fn query_get_account_balance(deps: Deps, address: String) -> StdResult<u128> {
    let addr = deps.api.addr_validate(address.as_str())?;
    let balance = BALANCE.load(deps.storage, &addr);
    match balance {
        Ok(balance) => Ok(balance.into()),
        Err(_) => Ok(0),
    }
}

fn query_get_round_price(deps: Deps, height: u64) -> StdResult<String> {
    let price = PRICES.load(deps.storage, height)?;
    Ok(price.to_string())
}

fn query_get_latest_price(deps: Deps) -> StdResult<Uint128> {
    let state = load_state(deps.storage)?;

    Ok(state.latest_price)
}

fn query_get_height_betting_list(deps: Deps, target_height: u64) -> StdResult<Vec<Betting>> {
    let bettings = BETTINGS.load(deps.storage, target_height);
    match bettings {
        Ok(bettings) => Ok(bettings),
        Err(_) => Ok(vec![]),
    }
}

fn query_get_recent_betting_list(deps: Deps, target_height: u64) -> StdResult<Vec<Betting>> {
    let mut bettings = vec![];
    for i in 0..=5 {
        let betting = BETTINGS
            .load(deps.storage, target_height - i)
            .unwrap_or_else(|_| vec![]);
        bettings.push(betting)
    }
    let bettings = bettings.into_iter().flatten().collect::<Vec<Betting>>();
    Ok(bettings)
}
fn query_state_lock(deps: Deps) -> StdResult<bool> {
    let state = load_state(deps.storage)?;
    match state.lock {
        true => Ok(true),
        false => Ok(false),
    }
}
