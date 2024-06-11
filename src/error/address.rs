use thiserror::Error;
#[derive(Error, Debug)]
pub enum AddrError {
    #[error("InvalidAddress")]
    InvalidAddress,
}
