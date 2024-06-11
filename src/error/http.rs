use thiserror::Error;
#[derive(Error, Debug)]
pub enum HttpError {
    #[error("Unable to build HTTP request")]
    BuildHttpReq,
    #[error("Unable to create HTTP request")]
    CreateHttpReq,
    #[error("Unable to send HTTP request")]
    SendHttpReq,
    #[error("Unable to receive HTTP response")]
    SendHttpRes,
    #[error("Unable to upgrade HTTP request: {0}")]
    UpgradeHttpReq(#[from] hyper::Error),
}
