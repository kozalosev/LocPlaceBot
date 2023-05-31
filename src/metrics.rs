use axum::routing::get;
use axum_prometheus::PrometheusMetricLayer;
use once_cell::sync::Lazy;
use prometheus::{Counter, Encoder, Opts, TextEncoder};

pub static INLINE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("inline_usage_total", "count of inline queries processed by the bot");
    Counter::with_opts(counter_opts).expect("unable to create the inline counter")
});
pub static INLINE_CHOSEN_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("inline_chosen_total", "count of inline results chosen by the users");
    Counter::with_opts(counter_opts).expect("unable to create the inline chosen counter")
});
pub static MESSAGE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("message_usage_total", "count of messages processed by the bot");
    Counter::with_opts(counter_opts).expect("unable to create the message counter")
});
pub static GOOGLE_API_REQUESTS_COUNTER: Lazy<Counter> = Lazy::new(|| {
    let counter_opts = Opts::new("google_maps_api_requests_total", "count of requests to the Google Maps API");
    Counter::with_opts(counter_opts).expect("unable to create the Google Maps API requests counter")
});

pub fn init() -> axum::Router {
    let prometheus = prometheus::Registry::new();
    prometheus.register(Box::new(INLINE_COUNTER.clone()))
        .expect("unable to register the inline counter");
    prometheus.register(Box::new(INLINE_CHOSEN_COUNTER.clone()))
        .expect("unable to register the inline chosen counter");
    prometheus.register(Box::new(MESSAGE_COUNTER.clone()))
        .expect("unable to register the message counter");
    prometheus.register(Box::new(GOOGLE_API_REQUESTS_COUNTER.clone()))
        .expect("unable to register the Google Maps API requests counter");

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
    axum::Router::new()
        .route("/metrics", get(|| async move {
            // Gather the metrics.
            let mut buffer = vec![];
            let metrics = prometheus.gather();
            TextEncoder::new().encode(&metrics, &mut buffer).unwrap();
            let custom_metrics = String::from_utf8(buffer).unwrap();

            metric_handle.render() + custom_metrics.as_str()
        }))
        .layer(prometheus_layer)
}
