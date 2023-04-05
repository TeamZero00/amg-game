use crate::error::ContractError;
use crate::helper::{betting_calculate, check_admin, check_denom, check_duration};
use crate::msg::{AMGBankMsg, ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::state::{load_state, save_state, Betting, Position, State, BALANCE, BETTINGS, PRICES};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, to_binary, BankMsg, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, Uint128, WasmMsg,
};

use cw2::set_contract_version;

use std::str::FromStr;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:fx-core";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let pool_contract = deps.api.addr_validate(&msg.pool_contract_address)?;
    let state = State {
        admin: info.sender.clone(),
        denom: msg.denom.clone(),
        fee_late: msg.fee_late,
        minimum_amount: Uint128::new(msg.minimum_amount as u128),
        pool_contract,
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
        ExecuteMsg::Setting { price } => setting(deps, env, info, price),
        ExecuteMsg::Claim {} => claim(deps, info),
        ExecuteMsg::SetFeeLate { fee_late } => set_fee_late(deps, env, info, fee_late),
        ExecuteMsg::SetMinimumAmount { amount } => set_minimum_amount(deps, env, info, amount),
        ExecuteMsg::SetBankContract { address } => set_bank_contract(deps, info, address),
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
    check_denom(&info, &state)?;
    check_duration(duration)?;
    let now_height = env.block.height;
    let denom = &info.funds[0];
    let base_price = Decimal::from_str(&PRICES.load(deps.storage, now_height)?)?;
    let denom_amount = denom.amount;
    //duration 확인

    let target_height = now_height + duration;

    //bank module 에서 돈 빌려와야함.

    //3/100 = 0.03
    let fee_late = Decimal::from_ratio(state.fee_late, Uint128::new(100));

    //수수료 금액
    let fee_amount: Uint128 = (fee_late * denom_amount).u128().into();

    //수수료 공제 금액
    let betting_amount = denom_amount - fee_amount;

    //option 업데이트
    {
        let position = Position::new(position.as_str())?;
        let betting = Betting::new(
            info.sender.clone(),
            position,
            betting_amount,
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

    let borrow_msg = AMGBankMsg::BorrowBalance {
        amount: betting_amount,
    };
    let fee_transfer_msg = AMGBankMsg::ProvideFee {};

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state.pool_contract.to_string(),
            msg: to_binary(&borrow_msg)?,
            funds: vec![],
        }))
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: state.pool_contract.to_string(),
            msg: to_binary(&fee_transfer_msg)?,
            funds: vec![coin(fee_amount.u128(), "uconst")],
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

fn setting(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    price: String,
) -> Result<Response, ContractError> {
    //
    let now_price = Decimal::from_str(price.as_str())?;
    // Load the contract state
    let state = load_state(deps.storage)?;

    check_admin(&info, &state)?;
    let now_height = env.block.height;

    // Save the new price
    /*
    If you set the price at the current block height,
    there is a possibility that users' now_price will be different,
    so we add + 1 to allow them to bet at the same price.
     */
    PRICES.save(deps.storage, now_height + 1, &price)?;

    let bettings = BETTINGS
        .load(deps.storage, now_height)
        .unwrap_or_else(|_| vec![]);

    let attrs = vec![
        ("method".to_string(), "setting".to_string()),
        ("now_price".to_string(), price.clone()),
    ];

    let mut return_balance = Uint128::new(0);

    if !bettings.is_empty() {
        return_balance = betting_calculate(&bettings, &mut deps, now_price)?;
    }

    BETTINGS.remove(deps.storage, now_height);

    let prize_attrs = bettings
        .into_iter()
        .map(|option| {
            (
                option.addr.to_string(),
                option
                    .amount
                    .checked_mul(Uint128::new(2))
                    .unwrap_or_else(|_| Uint128::MAX)
                    .to_string(),
            )
        })
        .collect::<Vec<(String, String)>>();
    let response_attrs = attrs
        .into_iter()
        .chain(prize_attrs)
        .collect::<Vec<(String, String)>>();

    let response = match return_balance.is_zero() {
        true => Response::new().add_attributes(response_attrs),
        false => Response::new()
            .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: state.pool_contract.to_string(),
                msg: to_binary(&AMGBankMsg::PayBack {})?,
                funds: vec![coin(return_balance.u128(), "uconst")],
            }))
            .add_attributes(response_attrs),
    };

    Ok(response)
}

fn claim(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let state = load_state(deps.storage)?;
    let balance = BALANCE.load(deps.storage, &info.sender)?;

    let coin = coin(balance.u128(), state.denom);

    let msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![coin],
    };

    let cos_msg = CosmosMsg::Bank(msg);

    BALANCE.remove(deps.storage, &info.sender);

    // Create a response with the `CosmosMsg::Bank` message
    let response = Response::new()
        .add_message(cos_msg)
        .add_attribute("method", "claim")
        .add_attribute("account", info.sender)
        .add_attribute("amount", balance.to_string());
    Ok(response)
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
    state.pool_contract = pool_contract;
    save_state(deps.storage, &state)?;

    Ok(Response::new().add_attribute("bank_contract", address))
}

// ######## TODO!!! Oracle version Setting

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetBalance { address } => to_binary(&query_get_account_balance(deps, address)?),
        QueryMsg::GetRoundPrice { height } => to_binary(&query_get_round_price(deps, height)?),
        QueryMsg::GetLatestPrice {} => to_binary(&query_get_latest_price(deps, env)?),
        QueryMsg::GetHeightBettingList { target_height } => {
            to_binary(&query_get_height_betting_list(deps, target_height)?)
        } // QueryMsg::GetLPContract {} => to_binary(&query_get_lp_contract(deps)),
    }
}

fn query_state(deps: Deps) -> StdResult<State> {
    let state = load_state(deps.storage)?;
    Ok(state)
}

fn query_get_account_balance(deps: Deps, address: String) -> StdResult<u128> {
    let addr = deps.api.addr_validate(address.as_str())?;
    let prize = BALANCE.load(deps.storage, &addr)?;
    Ok(prize.u128())
}

fn query_get_round_price(deps: Deps, height: u64) -> StdResult<String> {
    let price = PRICES.load(deps.storage, height)?;
    Ok(price)
}

fn query_get_latest_price(deps: Deps, env: Env) -> StdResult<String> {
    let price = PRICES.load(deps.storage, env.block.height - 1)?;
    Ok(price)
}

fn query_get_height_betting_list(deps: Deps, target_height: u64) -> StdResult<Vec<Betting>> {
    let bettings = BETTINGS.load(deps.storage, target_height)?;
    Ok(bettings)
}

// // This test passes only if check_enough_pool(this function is 310) is excluded from the betting function.
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use cosmwasm_std::testing::{
//         mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
//     };
//     use cosmwasm_std::{coins, from_binary};

//     fn do_instantiate(mut deps: DepsMut, init: InstantiateMsg) {
//         let info = mock_info("creator", &[]);

//         let env = mock_env();
//         let res = instantiate(deps.branch(), env.clone(), info.clone(), init).unwrap();
//         deposit(deps.branch(), info).unwrap();
//         assert_eq!(0, res.messages.len());
//         let config = query_config(deps.as_ref()).unwrap();
//         let admin = deps.api.addr_validate("creator").unwrap();
//         assert_eq!(
//             config,
//             State {
//                 admin: vec![admin],
//                 fee_late: 3,
//                 denom: "uconst".to_string(),
//                 minimum_amount: Uint128::new(100),
//                 last_height: env.block.height,
//             }
//         );
//         let pool = query_pool(deps.as_ref()).unwrap();
//         let lp_contarct_address = deps.api.addr_validate("lpcontract").unwrap();

//         assert_eq!(
//             pool,
//             Pool {
//                 asset: "uconst".to_string(),
//                 balance: Uint128::new(0),
//                 lp_contarct_address,
//                 option_game_total: Uint128::new(0),
//             }
//         );
//     }

//     #[test]
//     fn test_init() {
//         let mut deps = mock_dependencies();
//         let instantiate_msg = InstantiateMsg {
//             denom: "uconst".to_string(),
//             fee_late: 3,
//             price: 1000.to_string(),
//             minimum_amount: 100,
//             lp_contract_address: "lpcontract".to_string(),
//         };
//         do_instantiate(deps.as_mut(), instantiate_msg);
//     }
//     mod betting {
//         use super::*;
//         pub fn betting(
//             deps: DepsMut,
//             env: Env,
//             sender: &str,
//             position: &str,
//             amount: u128,
//             duration: u64,
//         ) {
//             let info = mock_info(sender, &[coin(amount, "uconst")]);
//             let msg = ExecuteMsg::Betting {
//                 position: position.to_string(),
//                 duration,
//             };

//             let _res = execute(deps, env, info, msg).unwrap();
//         }
//         #[test]
//         fn test_invalid_betting() {
//             let instantiate_msg = InstantiateMsg {
//                 denom: "uconst".to_string(),
//                 fee_late: 3,
//                 price: 1000.to_string(),
//                 minimum_amount: 100,
//                 lp_contract_address: "lpcontract".to_string(),
//             };
//             let mut deps = mock_dependencies();
//             do_instantiate(deps.as_mut(), instantiate_msg);

//             let info = mock_info("a", &[coin(10000, "uconst")]);
//             let msg = ExecuteMsg::Betting {
//                 position: "long".to_string(),
//                 duration: 1000,
//             };
//             let env = mock_env();
//             let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
//             //Invalid Duration Test
//             assert!(matches!(err, ContractError::InvalidDuration {}));

//             //minimum betting test
//             let info = mock_info("a", &[coin(10, "uconst")]);
//             let msg = ExecuteMsg::Betting {
//                 position: "long".to_string(),
//                 duration: 12,
//             };
//             let env = mock_env();
//             let err = execute(deps.as_mut(), env, info, msg).unwrap_err();

//             assert!(matches!(err, ContractError::InvalidMinimumAmount {}));

//             //invalid denom
//             let info = mock_info("a", &[coin(1_000_000_000, "btc")]);
//             let msg = ExecuteMsg::Betting {
//                 position: "long".to_string(),
//                 duration: 12,
//             };
//             let env = mock_env();
//             let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
//             assert!(matches!(err, ContractError::InvalidDenom {}));

//             //
//         }

//         #[test]
//         fn test_valid_betting() {
//             let instantiate_msg = InstantiateMsg {
//                 denom: "uconst".to_string(),
//                 fee_late: 3,
//                 price: 1000.to_string(),
//                 minimum_amount: 100,
//                 lp_contract_address: "lpcontract".to_string(),
//             };
//             let env = mock_env();
//             let height = &env.block.height;
//             let mut deps = mock_dependencies();

//             do_instantiate(deps.as_mut(), instantiate_msg);

//             betting(deps.as_mut(), env.clone(), "user1", "long", 10000, 12);
//             let pool = query_pool(deps.as_ref()).unwrap();
//             let options = query_get_height_betting_list(deps.as_ref(), *height + 12).unwrap();
//             assert_eq!(pool.balance, Uint128::new(10000));

//             let addr = deps.as_mut().api.addr_validate("user1").unwrap();
//             assert_eq!(
//                 options[0],
//                 Option {
//                     addr,
//                     start_height: *height,
//                     target_height: *height + 12,
//                     position: Position::Long,
//                     now_price: String::from("1000"),
//                     amount: Uint128::new(10000),
//                 }
//             );
//             let env = mock_env();
//             betting(deps.as_mut(), env.clone(), "user2", "short", 100_000, 12);
//             let pool = query_pool(deps.as_ref()).unwrap();
//             assert_eq!(pool.balance, Uint128::new(110000));
//             let options =
//                 query_get_height_betting_list(deps.as_ref(), env.block.height + 12).unwrap();
//             println!("{:#?}", options);
//             let addr = deps.as_mut().api.addr_validate("user2").unwrap();
//             assert_eq!(
//                 options[1],
//                 Option {
//                     addr,
//                     start_height: env.block.height,
//                     target_height: env.block.height + 12,
//                     position: Position::Short,
//                     now_price: String::from("1000"),
//                     amount: Uint128::new(100_000),
//                 }
//             );
//         }

//         fn setting(deps: DepsMut, mut env: Env, price: String) {
//             let info = mock_info("creator", &[]);
//             env.block.height += 1;
//             let msg = ExecuteMsg::Setting { price };
//             let _res = execute(deps, env, info, msg).unwrap();
//         }
//         #[test]
//         fn test_setting() {
//             let instantiate_msg = InstantiateMsg {
//                 denom: "uconst".to_string(),
//                 fee_late: 3,
//                 price: 1000.to_string(),
//                 minimum_amount: 100,
//                 lp_contract_address: "lpcontract".to_string(),
//             };
//             let mut deps = mock_dependencies();
//             do_instantiate(deps.as_mut(), instantiate_msg);
//             let mut env = mock_env();
//             setting(deps.as_mut(), env.clone(), 10000.to_string());
//             env.block.height += 1;
//             // println!("{:?}", env);

//             betting(deps.as_mut(), env.clone(), "user1", "short", 100_000, 12);
//             betting(deps.as_mut(), env.clone(), "user2", "long", 100_000, 12);
//             betting(deps.as_mut(), env.clone(), "user3", "long", 100_000, 12);

//             let mut env = mock_env();
//             env.block.height += 12;
//             // println!("{:?}", env);
//             setting(deps.as_mut(), env, 1000.to_string());
//             let pool = query_pool(deps.as_ref()).unwrap();

//             assert_eq!(pool.balance, Uint128::new(106000));
//             let prize = query_get_account_balance(deps.as_ref(), "user1".to_string()).unwrap();

//             assert_eq!(prize, Uint128::new(194000).u128());
//         }

//         // pub fn deposit(deps: DepsMut, sender: &str, amount: u128) {
//         //     let info = mock_info(sender, &[coin(amount, "uconst")]);
//         //     let msg = ExecuteMsg::Deposit {};
//         //     let env = mock_env();
//         //     let _res = execute(deps, env.clone(), info, msg).unwrap();
//         // }

//         // #[test]
//         // fn test_deposit() {
//         //     let instantiate_msg = InstantiateMsg {
//         //         denom: "uconst".to_string(),
//         //         fee_late: 3,
//         //         price: 1000,
//         //         minimum_amount: 100,
//         //         lp_contract_address: "lpcontract".to_string(),
//         //     };

//         //     let mut deps = mock_dependencies();

//         //     do_instantiate(deps.as_mut(), instantiate_msg);
//         //     deposit(deps.as_mut(), "user1", 10_000);
//         //     let pool = query_pool(deps.as_ref()).unwrap();
//         //     assert_eq!(pool.balance, Uint128::new(10_000));
//         // }
//     }
// }
