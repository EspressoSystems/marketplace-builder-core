//! This module implements interfaces necessary to run marketplace builder
//! in HotShot testing harness.

use std::{collections::HashMap, fmt::Display, marker::PhantomData, sync::Arc, time::Duration};

use async_compatibility_layer::art::async_spawn;
use async_trait::async_trait;
use hotshot::types::SignatureKey;
use hotshot_testing::{
    block_builder::{BuilderTask, TestBuilderImplementation},
    test_builder::BuilderChange,
};
use hotshot_types::{
    data::ViewNumber,
    traits::{node_implementation::NodeType, signature_key::BuilderSignatureKey},
};
use tagged_base64::TaggedBase64;
use url::Url;
use vbs::version::StaticVersion;

use crate::{
    hooks::{BuilderHooks, NoHooks},
    service::GlobalState,
};

const BUILDER_CHANNEL_CAPACITY: usize = 1024;

/// Testing configuration for marketplace builder
/// Stores hooks that will be used in the builder in a type-erased manner,
/// allowing for runtime configuration of hooks to use in tests.
struct TestMarketplaceBuilderConfig<Types>
where
    Types: NodeType,
{
    hooks: Box<dyn BuilderHooks<Types>>,
}

impl<Types> Default for TestMarketplaceBuilderConfig<Types>
where
    Types: NodeType,
{
    fn default() -> Self {
        Self {
            hooks: Box::new(NoHooks(PhantomData)),
        }
    }
}

/// [`TestBuilderImplementation`] for marketplace builder.
/// Passed as a generic parameter to [`TestRunner::run_test`], it is be used
/// to instantiate builder API and builder task.
struct MarketplaceBuilderImpl {}

#[async_trait]
impl<Types> TestBuilderImplementation<Types> for MarketplaceBuilderImpl
where
    Types: NodeType<View = ViewNumber>,
    Types::InstanceState: Default,
    for<'a> <<Types::SignatureKey as SignatureKey>::PureAssembledSignatureType as TryFrom<
        &'a TaggedBase64,
    >>::Error: Display,
    for<'a> <Types::SignatureKey as TryFrom<&'a TaggedBase64>>::Error: Display,
{
    type Config = TestMarketplaceBuilderConfig<Types>;

    /// This is mostly boilerplate to instantiate and start [`ProxyGlobalState`] APIs and initial [`BuilderState`]'s event loop.
    /// [`BuilderTask`] it returns will be injected into consensus runtime by HotShot testing harness and
    /// will forward transactions from hotshot event stream to the builder.
    async fn start(
        _n_nodes: usize,
        url: Url,
        config: Self::Config,
        _changes: HashMap<u64, BuilderChange>,
    ) -> Box<dyn BuilderTask<Types>> {
        let builder_key_pair = Types::BuilderSignatureKey::generated_from_seed_indexed([0; 32], 0);

        let service = Arc::new(GlobalState::new(
            builder_key_pair,
            Duration::from_millis(500),
            Duration::from_millis(10),
            Duration::from_secs(60),
            BUILDER_CHANNEL_CAPACITY,
            1, // Arbitrary base fee
            config.hooks,
        ));

        // create the proxy global state it will server the builder apis
        let app = service
            .clone()
            .into_app()
            .expect("Failed to create builder tide-disco app");

        let url_clone = url.clone();

        async_spawn(app.serve(url_clone, StaticVersion::<0, 1> {}));

        Box::new(MarketplaceBuilderTask { service })
    }
}

/// Marketplace builder task. Stores all the necessary information to run builder service
struct MarketplaceBuilderTask<Types>
where
    Types: NodeType,
{
    service: Arc<GlobalState<Types, Box<dyn BuilderHooks<Types>>>>,
}

impl<Types> BuilderTask<Types> for MarketplaceBuilderTask<Types>
where
    Types: NodeType<View = ViewNumber>,
    for<'a> <<Types::SignatureKey as SignatureKey>::PureAssembledSignatureType as TryFrom<
        &'a TaggedBase64,
    >>::Error: Display,
    for<'a> <Types::SignatureKey as TryFrom<&'a TaggedBase64>>::Error: Display,
{
    fn start(
        self: Box<Self>,
        stream: Box<
            dyn futures::prelude::Stream<Item = hotshot::types::Event<Types>>
                + std::marker::Unpin
                + Send
                + 'static,
        >,
    ) {
        self.service.start_event_loop(stream);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::testing::integration::MarketplaceBuilderImpl;
    use marketplace_builder_shared::testing::{
        generation::{self, TransactionGenerationConfig},
        run_test,
        validation::BuilderValidationConfig,
    };

    use hotshot_example_types::node_types::MarketplaceTestVersions;
    use hotshot_example_types::node_types::{MemoryImpl, TestTypes};
    use hotshot_macros::cross_tests;
    use hotshot_testing::{
        completion_task::{CompletionTaskDescription, TimeBasedCompletionTaskDescription},
        overall_safety_task::OverallSafetyPropertiesDescription,
        test_builder::TestDescription,
    };

    #[cfg_attr(async_executor_impl = "tokio", tokio::test(flavor = "multi_thread"))]
    #[cfg_attr(async_executor_impl = "async-std", async_std::test)]
    #[tracing::instrument]
    #[ignore = "slow"]
    async fn example_test() {
        let num_successful_views = 45;
        let min_txns_per_view = 5;

        run_test::<MarketplaceTestVersions, MarketplaceBuilderImpl>(
            TestDescription {
                completion_task_description:
                    CompletionTaskDescription::TimeBasedCompletionTaskBuilder(
                        TimeBasedCompletionTaskDescription {
                            duration: Duration::from_secs(60),
                        },
                    ),
                overall_safety_properties: OverallSafetyPropertiesDescription {
                    num_successful_views,
                    num_failed_views: 5,
                    ..Default::default()
                },
                ..TestDescription::default()
            },
            BuilderValidationConfig {
                expected_txn_num: num_successful_views * min_txns_per_view,
            },
            TransactionGenerationConfig {
                strategy: generation::GenerationStrategy::Random {
                    min_per_view: min_txns_per_view,
                    max_per_view: 10,
                    min_tx_size: 128,
                    max_tx_size: 1280,
                },
                endpoints: vec![],
            },
        )
        .await
    }

    #[cfg_attr(async_executor_impl = "tokio", tokio::test(flavor = "multi_thread"))]
    #[cfg_attr(async_executor_impl = "async-std", async_std::test)]
    #[tracing::instrument]
    #[ignore = "slow"]
    async fn stress_test() {
        run_test::<MarketplaceTestVersions, MarketplaceBuilderImpl>(
            TestDescription {
                completion_task_description:
                    CompletionTaskDescription::TimeBasedCompletionTaskBuilder(
                        TimeBasedCompletionTaskDescription {
                            duration: Duration::from_secs(60),
                        },
                    ),
                overall_safety_properties: OverallSafetyPropertiesDescription {
                    num_successful_views: 50,
                    num_failed_views: 5,
                    ..Default::default()
                },
                ..TestDescription::default()
            },
            BuilderValidationConfig {
                expected_txn_num: num_cpus::get() * 50,
            },
            TransactionGenerationConfig {
                strategy: generation::GenerationStrategy::Flood {
                    min_tx_size: 16,
                    max_tx_size: 128,
                },
                endpoints: vec![],
            },
        )
        .await
    }

    cross_tests!(
        TestName: example_cross_test,
        Impls: [MemoryImpl],
        BuilderImpls: [MarketplaceBuilderImpl],
        Types: [TestTypes],
        Versions: [MarketplaceTestVersions],
        Ignore: true,
        Metadata: {
            TestDescription {
                validate_transactions : hotshot_testing::test_builder::nonempty_block_threshold((90,100)),
                txn_description : hotshot_testing::txn_task::TxnTaskDescription::RoundRobinTimeBased(Duration::from_millis(10)),
                completion_task_description : CompletionTaskDescription::TimeBasedCompletionTaskBuilder(
                            TimeBasedCompletionTaskDescription {
                                duration: Duration::from_secs(120),
                            },
                ),
                overall_safety_properties: OverallSafetyPropertiesDescription {
                    num_successful_views: 50,
                    num_failed_views: 5,
                    ..Default::default()
                },
                ..Default::default()
            }
        },
    );
}
