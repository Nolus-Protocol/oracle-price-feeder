use thiserror::Error;
use tonic::codegen::http;

use crate::{
    cosmos::error::{CosmosError, WalletError},
    provider::FeedProviderError,
};

#[derive(Error, Debug)]
pub enum FeederError {
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    #[error("Invalid contract address {address}")]
    InvalidOracleContractAddress { address: String },

    #[error("Authentication error. Cause {message}")]
    AuthError { message: String },

    #[error("{0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("{0}")]
    InvalidUri(#[from] http::uri::InvalidUri),

    #[error("{0}")]
    StdError(#[from] std::io::Error),

    #[error("{0}")]
    WalletError(#[from] WalletError),

    #[error("{0}")]
    Provider(#[from] FeedProviderError),

    #[error("{0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Cosmos(#[from] CosmosError),
}
