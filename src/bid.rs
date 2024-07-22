use std::fs::File;
use std::io::Read;

use espresso_types::{
    v0_3::{BidTx, BidTxBody},
    FeeAccount, FeeAmount, NamespaceId,
};
use ethers::types::U256;
use hotshot_builder_api::v0_3::builder::BuildError;
use hotshot_types::{data::ViewNumber, traits::signature_key::BuilderSignatureKey};
use serde::Deserialize;
use serde_json::from_str;
use url::Url;

/// Configurations for the bid construction.
///
/// See `bid-config.rs` for an example.
#[derive(Clone, Debug, Deserialize)]
pub struct BidConfig {
    account_seed: [u8; 32],
    account_index: u64,
    gas_price: U256,
    bid_amount: U256,
    namespaces: Vec<u32>,
}

/// Read the bid configuration file.
pub fn read_bid_config_file(file_path: &str) -> Result<BidConfig, serde_json::Error> {
    let mut file = File::open(file_path).expect("Failed to open bid config file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Failed to read bid config file");
    from_str(&contents)
}

/// Construct a bid transaction from bid configurations.
///
/// Bid configurations can be found via `read_bid_config_file`.
pub fn from_bid_config(
    bid_config: BidConfig,
    view_number: ViewNumber,
    bid_base_url: Url,
) -> Result<BidTx, BuildError> {
    let (account, key) =
        FeeAccount::generated_from_seed_indexed(bid_config.account_seed, bid_config.account_index);
    let gas_price = FeeAmount(bid_config.gas_price);
    let bid_amount = FeeAmount(bid_config.bid_amount);
    let url = Url::parse(&format!("{}/{:?}/bundle", bid_base_url, view_number)).map_err(|e| {
        BuildError::Error {
            message: "Failed to parse the bid URL".to_string(),
        }
    })?;
    let namespaces = bid_config
        .namespaces
        .into_iter()
        .map(|id| NamespaceId::from(id))
        .collect();

    BidTxBody {
        account,
        gas_price,
        bid_amount,
        url,
        view: view_number,
        namespaces,
    }
    .signed(key)
    .map_err(|e| BuildError::Error {
        message: "Failed to sign the bid".to_string(),
    })
}
