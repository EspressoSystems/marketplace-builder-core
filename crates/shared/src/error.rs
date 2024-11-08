use std::sync::Arc;

use async_broadcast::TrySendError;
use hotshot::traits::BlockPayload;
use hotshot_builder_api::v0_3::builder::BuildError;
use hotshot_types::traits::{node_implementation::NodeType, signature_key::BuilderSignatureKey};
use thiserror::Error;
use tokio::time::error::Elapsed;

use crate::block::ReceivedTransaction;

#[derive(Error, Debug)]
pub enum Error<Types: NodeType> {
    #[error("Signature validation failed")]
    SignatureValidation,
    #[error("Failed to sign response")]
    Signing(<Types::BuilderSignatureKey as BuilderSignatureKey>::SignError),
    #[error("API response timed out")]
    ApiTimeout(#[from] Elapsed),
    #[error("Resource not found")]
    NotFound,
    #[error(transparent)]
    BuildBlock(<Types::BlockPayload as BlockPayload<Types>>::Error),
    #[error(transparent)]
    TxnSender(TrySendError<Arc<ReceivedTransaction<Types>>>),
}

impl<Types: NodeType> From<Error<Types>> for BuildError {
    fn from(value: Error<Types>) -> Self {
        match value {
            Error::SignatureValidation => {
                BuildError::Error("Signature validation failed".to_owned())
            }
            Error::Signing(_) => BuildError::Error("Failed to sign response".to_owned()),
            Error::ApiTimeout(_) => BuildError::Error("Timeout".to_owned()),
            Error::NotFound => BuildError::NotFound,
            Error::BuildBlock(_) => BuildError::Error("Failed to build block".to_owned()),
            Error::TxnSender(_) => BuildError::Error("Transaction channel error".to_owned()),
        }
    }
}
