use axum::routing::get;
use axum_prometheus::PrometheusMetricLayer;
use once_cell::sync::Lazy;
use prometheus::{Encoder, Opts, TextEncoder};

pub static INLINE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("inline", Opts::new("inline_usage_total", "count of inline queries processed by the bot"))
});
pub static INLINE_CHOSEN_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("inline_chosen", Opts::new("inline_chosen_total", "count of inline results chosen by the users"))
});
pub static MESSAGE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("message", Opts::new("message_usage_total", "count of messages processed by the bot"))
});

pub static GOOGLE_GEO_REQ_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("google_maps_api_requests_total", "count of requests to the Google Maps API")
        .const_label("API", "geocode");
    Counter::new("Google Maps API (geocode) requests", counter_opts)
});
pub static GOOGLE_PLACES_REQ_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("google_maps_api_requests_total", "count of requests to the Google Maps API")
        .const_label("API", "place");
    Counter::new("Google Maps API (place) requests", counter_opts)
});
pub static GOOGLE_PLACES_TEXT_REQ_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("google_maps_api_requests_total", "count of requests to the Google Maps API")
        .const_label("API", "place-text");
    Counter::new("Google Maps API (place, text) requests", counter_opts)
});

pub fn init() -> axum::Router {
    let prometheus = Registry(prometheus::Registry::new())
        .register(&*INLINE_COUNTER)
        .register(&*INLINE_CHOSEN_COUNTER)
        .register(&*MESSAGE_COUNTER)
        .register(&*GOOGLE_GEO_REQ_COUNTER)
        .register(&*GOOGLE_PLACES_REQ_COUNTER)
        .register(&*GOOGLE_PLACES_TEXT_REQ_COUNTER)
        .unwrap();

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
    axum::Router::new()
        .route("/metrics", get(|| async move {
            let mut buffer = vec![];
            let metrics = prometheus.gather();
            TextEncoder::new().encode(&metrics, &mut buffer).unwrap();
            let custom_metrics = String::from_utf8(buffer).unwrap();

            metric_handle.render() + custom_metrics.as_str()
        }))
        .layer(prometheus_layer)
}

pub struct Counter {
    inner: prometheus::Counter,
    name: String
}
struct Registry(prometheus::Registry);

impl Counter {
    fn new(name: &str, opts: Opts) -> Counter {
        let c = prometheus::Counter::with_opts(opts)
            .expect(format!("unable to create {name} counter").as_str());
        Counter { inner: c, name: name.to_string() }
    }

    pub fn inc(&self) {
        self.inner.inc()
    }
}

impl Registry {
    fn register(&self, counter: &Counter) -> &Self {
        self.0.register(Box::new(counter.inner.clone()))
            .expect(format!("unable to register the {} counter", counter.name).as_str());
        self
    }

    fn unwrap(&self) -> prometheus::Registry {
        self.0.clone()
    }
}
