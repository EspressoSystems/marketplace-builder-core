use std::time::Duration;

use async_compatibility_layer::art::async_sleep;
use marketplace_builder_shared::{
    block::{BuilderStateId, ReceivedTransaction, TransactionSource},
    coordinator::{BuilderStateCoordinator, BuilderStateLookup},
    state::BuilderState,
};

pub use async_broadcast::{broadcast, RecvError, TryRecvError};
use async_lock::RwLock;
#[cfg(async_executor_impl = "async-std")]
use async_std::task::JoinHandle;
use async_trait::async_trait;
use committable::{Commitment, Committable};
use futures::{future::BoxFuture, stream::FuturesUnordered, Stream};
use futures::{
    stream::{FuturesOrdered, StreamExt},
    TryStreamExt,
};
use hotshot::types::Event;
use hotshot_builder_api::v0_3::{
    builder::{define_api, submit_api, BuildError, Error as BuilderApiError},
    data_source::{AcceptsTxnSubmits, BuilderDataSource},
};
use hotshot_types::bundle::Bundle;
use hotshot_types::traits::block_contents::{BuilderFee, Transaction};
use hotshot_types::{
    event::EventType,
    traits::{
        node_implementation::{ConsensusTime, NodeType},
        signature_key::{BuilderSignatureKey, SignatureKey},
    },
    vid::VidCommitment,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::{fmt::Display, time::Instant};
use tagged_base64::TaggedBase64;
use tide_disco::{app::AppError, method::ReadState, App};
#[cfg(async_executor_impl = "tokio")]
use tokio::task::JoinHandle;
use tracing::Level;
use vbs::version::StaticVersion;

pub use marketplace_builder_shared::utils::EventServiceStream;

use crate::hooks::BuilderHooks;

pub struct GlobalState<Types, Hooks>
where
    Types: NodeType,
    Hooks: BuilderHooks<Types>,
{
    // global state
    coordinator: Arc<BuilderStateCoordinator<Types>>,

    // identity keys for the builder
    // May be ideal place as GlobalState interacts with hotshot apis
    // and then can sign on responders as desired
    builder_keys: (
        Types::BuilderSignatureKey, // pub key
        <<Types as NodeType>::BuilderSignatureKey as BuilderSignatureKey>::BuilderPrivateKey, // private key
    ),

    // Maximum time allotted to wait for bundle before returning an error
    api_timeout: Duration,

    maximize_txn_capture_timeout: Duration,

    bundle_cache: RwLock<HashMap<BuilderStateId<Types>, Bundle<Types>>>,

    base_fee: u64,

    hooks: Arc<Hooks>,
}

impl<Types, Hooks> GlobalState<Types, Hooks>
where
    Types: NodeType,
    Hooks: BuilderHooks<Types>,
    for<'a> <<Types::SignatureKey as SignatureKey>::PureAssembledSignatureType as TryFrom<
        &'a TaggedBase64,
    >>::Error: Display,
    for<'a> <Types::SignatureKey as TryFrom<&'a TaggedBase64>>::Error: Display,
{
    pub fn new(
        builder_keys: (
            Types::BuilderSignatureKey,
            <<Types as NodeType>::BuilderSignatureKey as BuilderSignatureKey>::BuilderPrivateKey,
        ),
        api_timeout: Duration,
        maximize_txn_capture_timeout: Duration,
        txn_garbage_collect_duration: Duration,
        txn_channel_capacity: usize,
        base_fee: u64,
        hooks: Hooks,
    ) -> Self {
        let coordinator =
            BuilderStateCoordinator::new(txn_channel_capacity, txn_garbage_collect_duration);
        Self {
            hooks: Arc::new(hooks),
            coordinator: Arc::new(coordinator),
            builder_keys,
            api_timeout,
            maximize_txn_capture_timeout,
            bundle_cache: RwLock::new(HashMap::new()),
            base_fee,
        }
    }

    /// Consumes `self` and returns a `tide_disco` [`App`] with builder and private mempool APIs registered
    pub fn into_app(
        self: Arc<Self>,
    ) -> Result<App<ProxyGlobalState<Types, Hooks>, BuilderApiError>, AppError> {
        let proxy = ProxyGlobalState(self);
        let builder_api = define_api::<ProxyGlobalState<Types, Hooks>, Types>(&Default::default())?;

        // TODO: Replace StaticVersion with proper constant when added in HotShot
        let private_mempool_api =
            submit_api::<ProxyGlobalState<Types, Hooks>, Types, StaticVersion<0, 1>>(
                &Default::default(),
            )?;

        let mut app: App<ProxyGlobalState<Types, Hooks>, BuilderApiError> = App::with_state(proxy);

        app.register_module(
            hotshot_types::constants::MARKETPLACE_BUILDER_MODULE,
            builder_api,
        )?;

        app.register_module("txn_submit", private_mempool_api)?;

        Ok(app)
    }

    pub fn start_event_loop(
        &self,
        event_stream: impl Stream<Item = Event<Types>> + Unpin + Send + 'static,
    ) -> JoinHandle<anyhow::Result<()>> {
        async_compatibility_layer::art::async_spawn(Self::drive_coordinator(
            self.coordinator.clone(),
            self.hooks.clone(),
            event_stream,
        ))
    }

    async fn drive_coordinator(
        coordinator: Arc<BuilderStateCoordinator<Types>>,
        hooks: Arc<Hooks>,
        mut event_stream: impl Stream<Item = Event<Types>> + Unpin + Send + 'static,
    ) -> anyhow::Result<()> {
        loop {
            let Some(event) = event_stream.next().await else {
                anyhow::bail!("Event stream ended");
            };

            hooks.handle_hotshot_event(&event).await;

            match event.event {
                EventType::Error { error } => {
                    tracing::error!("Error event in HotShot: {:?}", error);
                }
                EventType::Transactions { transactions } => {
                    let transactions = hooks.process_transactions(transactions).await;

                    // TODO: record results
                    let _ = transactions
                        .into_iter()
                        .map(|txn| {
                            coordinator.handle_transaction(ReceivedTransaction::new(
                                txn,
                                TransactionSource::Public,
                            ))
                        })
                        .collect::<FuturesUnordered<_>>()
                        .collect::<Vec<_>>()
                        .await;
                }
                EventType::Decide { leaf_chain, .. } => {
                    coordinator.handle_decide(leaf_chain).await;
                }
                EventType::DaProposal { proposal, .. } => {
                    coordinator.handle_da_proposal(proposal.data).await;
                }
                EventType::QuorumProposal { proposal, .. } => {
                    coordinator.handle_quorum_proposal(proposal.data).await;
                }
                _ => {}
            }
        }
    }

    /// Collect transactions to include in the bundle. Will wait until we have
    /// at least one transaction or up to the configured `maximize_txn_capture_timeout` duration elapses.
    #[tracing::instrument(skip_all, fields(builder_parent_block_references = %state.parent_block_references))]
    async fn collect_transactions(
        &self,
        state: &Arc<BuilderState<Types>>,
    ) -> Option<Vec<Types::Transaction>> {
        // collect all the transactions from the near future
        let timeout_after = Instant::now() + self.maximize_txn_capture_timeout;
        let sleep_interval = self.maximize_txn_capture_timeout / 10;
        while Instant::now() <= timeout_after {
            let queue_populated = state.collect_txns(timeout_after).await;

            if queue_populated || Instant::now() + sleep_interval > timeout_after {
                // we don't have time for another iteration
                break;
            }

            async_sleep(sleep_interval).await
        }

        let transactions = state
            .txn_queue
            .read()
            .await
            .transactions
            .iter()
            .map(|txn| txn.transaction.clone())
            .collect();

        Some(transactions)
    }

    async fn assemble_bundle(
        &self,
        transactions: Vec<Types::Transaction>,
    ) -> Result<Bundle<Types>, BuildError> {
        let bundle_size: u64 = transactions
            .iter()
            .map(|txn| txn.minimum_block_size())
            .sum();
        let offered_fee = self.base_fee * bundle_size;

        let fee_signature =
            <Types::BuilderSignatureKey as BuilderSignatureKey>::sign_sequencing_fee_marketplace(
                &self.builder_keys.1,
                offered_fee,
            )
            .map_err(|e| BuildError::Error(e.to_string()))?;

        let sequencing_fee: BuilderFee<Types> = BuilderFee {
            fee_amount: offered_fee,
            fee_account: self.builder_keys.0.clone(),
            fee_signature,
        };

        let commitments = transactions
            .iter()
            .flat_map(|txn| <[u8; 32]>::from(txn.commit()))
            .collect::<Vec<u8>>();

        let signature = <Types::BuilderSignatureKey as BuilderSignatureKey>::sign_builder_message(
            &self.builder_keys.1,
            &commitments,
        )
        .map_err(|e| BuildError::Error(e.to_string()))?;

        Ok(Bundle {
            sequencing_fee,
            transactions,
            signature,
        })
    }
}

#[derive(derive_more::Deref, derive_more::DerefMut)]
#[deref(forward)]
#[deref_mut(forward)]
pub struct ProxyGlobalState<Types, Hooks>(pub Arc<GlobalState<Types, Hooks>>)
where
    Types: NodeType,
    Hooks: BuilderHooks<Types>;

/*
Handling Builder API responses
*/
#[async_trait]
impl<Types, Hooks> BuilderDataSource<Types> for ProxyGlobalState<Types, Hooks>
where
    Types: NodeType,
    Hooks: BuilderHooks<Types>,
    for<'a> <<Types::SignatureKey as SignatureKey>::PureAssembledSignatureType as TryFrom<
        &'a TaggedBase64,
    >>::Error: Display,
    for<'a> <Types::SignatureKey as TryFrom<&'a TaggedBase64>>::Error: Display,
{
    #[tracing::instrument(
        skip(self),
        err(level = Level::INFO)
        ret(level = Level::TRACE)
    )]
    async fn bundle(
        &self,
        parent_view: u64,
        parent_hash: &VidCommitment,
        _view_number: u64,
    ) -> Result<Bundle<Types>, BuildError> {
        let start = Instant::now();

        let parent_view = Types::View::new(parent_view);
        let state_id = BuilderStateId {
            parent_view,
            parent_commitment: *parent_hash,
        };

        loop {
            // Couldn't serve a bundle in time
            if start.elapsed() > self.api_timeout {
                tracing::warn!("Timeout while trying to serve a bundle");
                return Err(BuildError::NotFound);
            };

            let builder_state = match self.coordinator.lookup_builder_state(&state_id).await {
                BuilderStateLookup::Found(builder_state_entry) => builder_state_entry,
                BuilderStateLookup::NotFound => {
                    // If we couldn't find the state because it hasn't yet been created, try again
                    async_sleep(self.api_timeout / 10).await;
                    continue;
                }
                BuilderStateLookup::Decided => {
                    // If we couldn't find the state because the view has already been decided, we can just return an error
                    tracing::warn!("Requested a bundle for view we already GCd as decided",);
                    return Err(BuildError::Error(
                        "Request for a bundle for a view that has already been decided.".to_owned(),
                    ));
                }
            };

            tracing::info!(
                "Request handled by builder with view {}@{:?} for (view_num: {:?})",
                builder_state.parent_block_references.vid_commitment,
                builder_state.parent_block_references.view_number,
                parent_view
            );

            let Some(transactions) = self.collect_transactions(&builder_state).await else {
                tracing::debug!("No response to send");
                return Err(BuildError::NotFound);
            };

            let bundle = self.assemble_bundle(transactions).await?;

            self.bundle_cache
                .write()
                .await
                .insert(state_id, bundle.clone());

            tracing::info!("Serving bundle");

            return Ok(bundle);
        }
    }

    async fn builder_address(
        &self,
    ) -> Result<<Types as NodeType>::BuilderSignatureKey, BuildError> {
        Ok(self.builder_keys.0.clone())
    }
}

#[async_trait]
impl<Types, Hooks> AcceptsTxnSubmits<Types> for ProxyGlobalState<Types, Hooks>
where
    Hooks: BuilderHooks<Types>,
    Types: NodeType,
{
    async fn submit_txns(
        &self,
        txns: Vec<<Types as NodeType>::Transaction>,
    ) -> Result<Vec<Commitment<<Types as NodeType>::Transaction>>, BuildError> {
        let txns = self.hooks.process_transactions(txns).await;

        txns.into_iter()
            .map(|txn| ReceivedTransaction::new(txn, TransactionSource::Private))
            .map(|txn| async {
                let commit = txn.commit;
                self.coordinator
                    .handle_transaction(txn)
                    .await
                    .map(|_| commit)
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
    }
}

#[async_trait]
impl<Types, Hooks> ReadState for ProxyGlobalState<Types, Hooks>
where
    Types: NodeType,
    Hooks: BuilderHooks<Types>,
{
    type State = Self;

    async fn read<T>(
        &self,
        op: impl Send + for<'a> FnOnce(&'a Self::State) -> BoxFuture<'a, T> + 'async_trait,
    ) -> T {
        op(self).await
    }
}
