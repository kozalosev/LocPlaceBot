use axum::routing::get;
use axum_prometheus::PrometheusMetricLayer;
use once_cell::sync::Lazy;
use prometheus::{Encoder, Opts, TextEncoder};

/// Register additional metrics of our own structs by using this registry instance.
pub static REGISTRY: Lazy<Registry> = Lazy::new(|| Registry(prometheus::Registry::new()));

// Export special preconstructed counters for Teloxide's handlers.
pub static INLINE_COUNTER: Lazy<InlineCounters> = Lazy::new(|| {
    let opts = Opts::new("inline_usage_total", "count of inline queries processed by the bot or rejected by the rate limiter");
    InlineCounters {
        allowed: Counter::new("inline (allowed)", opts.clone().const_label("limiter", "allowed")),
        forbidden: Counter::new("inline (forbidden)", opts.const_label("limiter", "forbidden")),
    }
});
pub static INLINE_CHOSEN_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("inline_chosen", Opts::new("inline_chosen_total", "count of inline results chosen by the users"))
});
pub static MESSAGE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("message", Opts::new("message_usage_total", "count of messages processed by the bot"))
});
pub static CMD_START_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("command_start", Opts::new("command_start_usage_total", "count of /start invocations"))
});
pub static CMD_HELP_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("command_help", Opts::new("command_help_usage_total", "count of /help invocations"))
});
pub static CMD_LOC_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("command_loc", Opts::new("command_loc_usage_total", "count of /loc invocations"))
});
pub static CMD_SET_LANGUAGE_COUNTER: Lazy<Counter> = Lazy::new(|| {
    Counter::new("command_set_language", Opts::new("command_set_language_usage_total", "count of /setlanguage invocations"))
});
pub static CMD_SET_LOCATION_COUNTER: Lazy<ComplexCommandCounters> = Lazy::new(|| {
    let opts = Opts::new("command_set_location_usage_total", "count of /setlocation invocations");
    ComplexCommandCounters {
        invoked: Counter::new("command_set_location (start)", opts.clone().const_label("state", "invoked")),
        finished: Counter::new("command_set_location (set)", opts.const_label("state", "finished")),
    }
});

pub fn init() -> axum::Router {
    let prometheus = REGISTRY
        .register(&INLINE_COUNTER.allowed)
        .register(&INLINE_COUNTER.forbidden)
        .register(&*INLINE_CHOSEN_COUNTER)
        .register(&*MESSAGE_COUNTER)
        .register(&*CMD_START_COUNTER)
        .register(&*CMD_HELP_COUNTER)
        .register(&*CMD_LOC_COUNTER)
        .register(&*CMD_SET_LANGUAGE_COUNTER)
        .register(&CMD_SET_LOCATION_COUNTER.invoked)
        .register(&CMD_SET_LOCATION_COUNTER.finished)
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
pub struct InlineCounters {
    allowed: Counter,
    forbidden: Counter,
}
pub struct ComplexCommandCounters {
    invoked: Counter,
    finished: Counter,
}
pub struct Registry(prometheus::Registry);

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

impl ComplexCommandCounters {
    pub fn invoked(&self) {
        self.invoked.inc()
    }

    pub fn finished(&self) {
        self.finished.inc()
    }
}

impl Registry {
    /// Register additional counters by our own structs.
    pub fn register_counter(&self, name: &str, opts: Opts) -> prometheus::Counter {
        let c = prometheus::Counter::with_opts(opts)
            .expect(format!("unable to create {name} counter").as_str());
        self.0.register(Box::new(c.clone()))
            .expect(format!("unable to register the {name} counter").as_str());
        c
    }

    fn register(&self, counter: &Counter) -> &Self {
        self.0.register(Box::new(counter.inner.clone()))
            .expect(format!("unable to register the {} counter", counter.name).as_str());
        self
    }

    fn unwrap(&self) -> prometheus::Registry {
        self.0.clone()
    }
}

impl InlineCounters {
    pub fn inc_allowed(&self) {
        self.allowed.inc()
    }

    pub fn inc_forbidden(&self) {
        self.forbidden.inc()
    }
}
