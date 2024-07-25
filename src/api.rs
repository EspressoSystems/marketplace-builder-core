use std::{fs, path::Path};

use anyhow::{Context, Error};
use futures::FutureExt;
use hotshot_builder_api::v0_3::{builder::Options, data_source::BuilderDataSource, Version};
use hotshot_task_impls::transactions::Bundle;
use hotshot_types::traits::{
    block_contents::BuilderFee, node_implementation::NodeType, signature_key::BuilderSignatureKey,
};
use tide_disco::{api::ApiError, method::ReadState, Api};
use toml::{map::Entry, Value};
use vbs::version::StaticVersionType;

fn load_toml(path: &Path) -> Result<Value, ApiError> {
    let bytes = fs::read(path).map_err(|err| ApiError::CannotReadToml {
        reason: err.to_string(),
    })?;
    let string = std::str::from_utf8(&bytes).map_err(|err| ApiError::CannotReadToml {
        reason: err.to_string(),
    })?;
    toml::from_str(string).map_err(|err| ApiError::CannotReadToml {
        reason: err.to_string(),
    })
}

fn merge_toml(into: &mut Value, from: Value) {
    if let (Value::Table(into), Value::Table(from)) = (into, from) {
        for (key, value) in from {
            match into.entry(key) {
                Entry::Occupied(mut entry) => merge_toml(entry.get_mut(), value),
                Entry::Vacant(entry) => {
                    entry.insert(value);
                }
            }
        }
    }
}

pub(crate) fn load_api<State: 'static, Error: 'static, Ver: StaticVersionType + 'static>(
    path: Option<impl AsRef<Path>>,
    default: &str,
    extensions: impl IntoIterator<Item = Value>,
) -> Result<Api<State, Error, Ver>, ApiError> {
    let mut toml = match path {
        Some(path) => load_toml(path.as_ref())?,
        None => toml::from_str(default).map_err(|err| ApiError::CannotReadToml {
            reason: err.to_string(),
        })?,
    };
    for extension in extensions {
        merge_toml(&mut toml, extension);
    }
    Api::new(toml)
}

pub fn define_api<State, TYPES: NodeType>(
    options: &Options,
) -> Result<Api<State, Error, Version>, ApiError>
where
    State: 'static + Send + Sync + ReadState,
    <State as ReadState>::State: Send + Sync + BuilderDataSource<TYPES>,
{
    let mut api = load_api::<State, Error, Version>(
        options.api_path.as_ref(),
        include_str!("builder.toml"),
        options.extensions.clone(),
    )?;
    api.with_version("0.0.3".parse().unwrap())
        .get("bundle", |_req, _state| {
            async move {
                // let view_number = req.integer_param("view_number")?;

                let (pub_key, priv_key) = <TYPES::BuilderSignatureKey as BuilderSignatureKey>::generated_from_seed_indexed(
                        [0_u8; 32], 0,
                    );
                const FEE_AMOUNT: u64 = 1;
                let signature = <TYPES::BuilderSignatureKey as BuilderSignatureKey>::sign_sequencing_fee_marketplace(&priv_key, FEE_AMOUNT).with_context(|| format!("Failed to sign the key"))?;
                let builder_fee:BuilderFee<TYPES> = BuilderFee {
                        fee_amount: FEE_AMOUNT,
                        fee_account: pub_key,
                        fee_signature: signature,
                    };

                Ok(())
                // TODO: Should bid_fee be removed and Bundle serializable?
                // Ok(Bundle {
                //     transactions: vec![],
                //     signature,
                //     bid_fee: builder_fee,
                //     sequencing_fee: builder_fee,
                // })
            }
            .boxed()
        })?;
    Ok(api)
}
