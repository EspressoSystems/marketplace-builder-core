use std::{future::Future, pin::Pin, task::Poll};

use std::{
    collections::HashSet,
    hash::Hash,
    mem,
    time::{Duration, Instant},
};

use async_compatibility_layer::art::async_sleep;
use either::Either::{self, Left, Right};
use futures::{ready, FutureExt, Stream, StreamExt};
use hotshot::types::Event;
use hotshot_events_service::events::Error as EventStreamError;
use hotshot_types::traits::node_implementation::NodeType;
use surf_disco::client::HealthStatus;
use surf_disco::Client;
use tracing::{error, warn};
use url::Url;
use vbs::version::StaticVersionType;

/// A set that allows for time-based garbage collection,
/// implemented as three sets that are periodically shifted right.
/// Garbage collection is triggered by calling [`Self::rotate`].
#[derive(Clone, Debug)]
pub struct RotatingSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    fresh: HashSet<T>,
    stale: HashSet<T>,
    expiring: HashSet<T>,
    last_rotation: Instant,
    period: Duration,
}

impl<T> RotatingSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    /// Construct a new `RotatingSet`
    pub fn new(period: Duration) -> Self {
        Self {
            fresh: HashSet::new(),
            stale: HashSet::new(),
            expiring: HashSet::new(),
            last_rotation: Instant::now(),
            period,
        }
    }

    /// Returns `true` if the key is contained in the set
    pub fn contains(&self, key: &T) -> bool {
        self.fresh.contains(key) || self.stale.contains(key) || self.expiring.contains(key)
    }

    /// Insert a `key` into the set. Doesn't trigger garbage collection
    pub fn insert(&mut self, value: T) {
        self.fresh.insert(value);
    }

    /// Force garbage collection, even if the time elapsed since
    ///  the last garbage collection is less than `self.period`
    pub fn force_rotate(&mut self) {
        let now_stale = mem::take(&mut self.fresh);
        let now_expiring = mem::replace(&mut self.stale, now_stale);
        self.expiring = now_expiring;
        self.last_rotation = Instant::now();
    }

    /// Trigger garbage collection.
    pub fn rotate(&mut self) -> bool {
        if self.last_rotation.elapsed() > self.period {
            self.force_rotate();
            true
        } else {
            false
        }
    }
}

impl<T> Extend<T> for RotatingSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.fresh.extend(iter)
    }
}

type EventServiceConnection<Types, ApiVer> = surf_disco::socket::Connection<
    Event<Types>,
    surf_disco::socket::Unsupported,
    EventStreamError,
    ApiVer,
>;

type EventServiceReconnect<Types, ApiVer> = Pin<
    Box<dyn Future<Output = anyhow::Result<EventServiceConnection<Types, ApiVer>>> + Send + Sync>,
>;

/// A wrapper around event streaming API that provides auto-reconnection capability
pub struct EventServiceStream<Types: NodeType, V: StaticVersionType> {
    api_url: Url,
    connection: Either<EventServiceConnection<Types, V>, EventServiceReconnect<Types, V>>,
}

impl<Types: NodeType, ApiVer: StaticVersionType> EventServiceStream<Types, ApiVer> {
    async fn connect_inner(
        url: Url,
    ) -> anyhow::Result<
        surf_disco::socket::Connection<
            Event<Types>,
            surf_disco::socket::Unsupported,
            EventStreamError,
            ApiVer,
        >,
    > {
        let client = Client::<hotshot_events_service::events::Error, ApiVer>::new(url.clone());

        loop {
            if client.healthcheck::<HealthStatus>().await.is_ok() {
                break;
            }
            async_sleep(Duration::from_secs(1)).await;
        }

        tracing::info!("Builder client connected to the hotshot events api");

        Ok(client
            .socket("hotshot-events/events")
            .subscribe::<Event<Types>>()
            .await?)
    }

    /// Establish initial connection to the events service at `api_url`
    pub async fn connect(api_url: Url) -> anyhow::Result<Self> {
        let connection = Self::connect_inner(api_url.clone()).await?;

        Ok(Self {
            api_url,
            connection: Left(connection),
        })
    }
}

impl<Types: NodeType, V: StaticVersionType + 'static> Stream for EventServiceStream<Types, V> {
    type Item = Event<Types>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match &mut self.connection {
            Left(connection) => {
                let next = ready!(connection.poll_next_unpin(cx));
                match next {
                    Some(Ok(event)) => Poll::Ready(Some(event)),
                    Some(Err(err)) => {
                        warn!(?err, "Error in event stream");
                        Poll::Pending
                    }
                    None => {
                        warn!("Event stream ended, attempting reconnection");
                        let fut = Self::connect_inner(self.api_url.clone());
                        let _ = std::mem::replace(&mut self.connection, Right(Box::pin(fut)));
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            Right(reconnect_future) => {
                let next = ready!(reconnect_future.poll_unpin(cx));
                match next {
                    Err(err) => {
                        error!(?err, "Error while reconnecting, giving up");
                        Poll::Ready(None)
                    }
                    Ok(connection) => {
                        let _ = std::mem::replace(&mut self.connection, Left(connection));
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        time::Duration,
    };

    use async_compatibility_layer::art::async_spawn;
    use async_std::prelude::FutureExt;
    #[cfg(async_executor_impl = "async-std")]
    use async_std::task::JoinHandle;
    use async_trait::async_trait;
    use futures::{future::BoxFuture, stream, StreamExt};
    use hotshot::types::{Event, EventType};
    use hotshot_events_service::{
        events::define_api,
        events_source::{EventFilterSet, EventsSource, StartupInfo},
    };
    use hotshot_example_types::node_types::TestTypes;
    use hotshot_types::{data::ViewNumber, traits::node_implementation::ConsensusTime};
    use tide_disco::{method::ReadState, App};
    #[cfg(async_executor_impl = "tokio")]
    use tokio::task::JoinHandle;
    use url::Url;
    use vbs::version::StaticVersion;

    use crate::utils::EventServiceStream;

    type MockVersion = StaticVersion<0, 1>;

    struct MockEventsSource {
        counter: AtomicU64,
    }

    #[async_trait]
    impl EventsSource<TestTypes> for MockEventsSource {
        type EventStream = futures::stream::Iter<std::vec::IntoIter<Arc<Event<TestTypes>>>>;

        async fn get_event_stream(
            &self,
            _filter: Option<EventFilterSet<TestTypes>>,
        ) -> Self::EventStream {
            let view = ViewNumber::new(self.counter.load(Ordering::SeqCst));
            let test_event = Arc::new(Event {
                view_number: view,
                event: EventType::ViewFinished { view_number: view },
            });
            self.counter.fetch_add(1, Ordering::SeqCst);
            stream::iter(vec![test_event])
        }

        async fn get_startup_info(&self) -> StartupInfo<TestTypes> {
            StartupInfo {
                known_node_with_stake: Vec::new(),
                non_staked_node_count: 0,
            }
        }
    }

    #[async_trait]
    impl ReadState for MockEventsSource {
        type State = Self;

        async fn read<T>(
            &self,
            op: impl Send + for<'a> FnOnce(&'a Self::State) -> BoxFuture<'a, T> + 'async_trait,
        ) -> T {
            op(self).await
        }
    }

    fn run_app(path: &'static str, bind_url: Url) -> JoinHandle<()> {
        let source = MockEventsSource {
            counter: AtomicU64::new(0),
        };
        let api = define_api::<MockEventsSource, _, MockVersion>(&Default::default()).unwrap();

        let mut app: App<MockEventsSource, hotshot_events_service::events::Error> =
            App::with_state(source);

        app.register_module(path, api).unwrap();

        async_spawn(async move { app.serve(bind_url, MockVersion {}).await.unwrap() })
    }

    #[cfg_attr(async_executor_impl = "tokio", tokio::test(flavor = "multi_thread"))]
    #[cfg_attr(async_executor_impl = "async-std", async_std::test)]
    async fn event_stream_wrapper() {
        async_compatibility_layer::logging::setup_logging();
        async_compatibility_layer::logging::setup_backtrace();

        const TIMEOUT: Duration = Duration::from_secs(3);

        let url: Url = format!(
            "http://localhost:{}",
            portpicker::pick_unused_port().unwrap()
        )
        .parse()
        .unwrap();

        let app_handle = run_app("hotshot-events", url.clone());

        let mut stream = EventServiceStream::<TestTypes, MockVersion>::connect(url.clone())
            .await
            .unwrap();

        stream
            .next()
            .timeout(TIMEOUT)
            .await
            .expect("When mock event server is spawned, stream should work");

        #[cfg(async_executor_impl = "tokio")]
        app_handle.abort();
        #[cfg(async_executor_impl = "async-std")]
        app_handle.cancel().await;

        stream
            .next()
            .timeout(TIMEOUT)
            .await
            .expect_err("When mock event server is killed, stream should be in reconnecting state and never return");

        let app_handle = run_app("hotshot-events", url.clone());

        stream
            .next()
            .timeout(TIMEOUT)
            .await
            .expect("When mock event server is restarted, stream should work again");

        #[cfg(async_executor_impl = "tokio")]
        app_handle.abort();
        #[cfg(async_executor_impl = "async-std")]
        app_handle.cancel().await;

        run_app("wrong-path", url.clone());

        assert!(
            stream
                .next()
                .timeout(TIMEOUT)
                .await
                .expect("API is reachable")
                .is_none(),
            "Stream should've ended, because while url is reachable, it doesn't conform to expeted API"
        )
    }
}
