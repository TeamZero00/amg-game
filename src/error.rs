use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Must Send Coin")]
    MustSendCoin {},

    #[error("Must Send One Coin")]
    InvalidOneTypeCoin {},

    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("Only Onwer")]
    OnlyOwner {},
    #[error("SAVE error ")]
    SaveError {},
    #[error("Position Invalid")]
    InvalidPosition {},

    #[error("Denom Invalid")]
    InvalidDenom {},

    #[error("Minimum Amount Invalid")]
    InvalidMinimumAmount {},

    #[error("Invalid duration")]
    InvalidDuration {},

    #[error("Invalid Height")]
    InvalidHeight {},

    #[error("Invalid LP Allowance")]
    InvalidLPAllowance {},

    #[error("pool balance must not be less than the options being played. ")]
    InvalidWithdrawBalance {},

    #[error("Expires is Invalid you must setting expires")]
    InvalidExpires {},

    #[error("This is more than the current pool can handle.")]
    NotEnoughPool {},

    #[error("You placed a bet over the lock height.")]
    OverDeadline {},
}
