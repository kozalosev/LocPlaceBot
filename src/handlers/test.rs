use super::{is_query_correct, COORDS_REGEXP, QUERY_REGEX};

mod otel {
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::{SdkTracerProvider, SimpleSpanProcessor};
    use tracing::Subscriber;
    use tracing_subscriber::{layer::SubscriberExt, Registry};

    pub fn setup_otel_test() -> (InMemorySpanExporter, SdkTracerProvider, impl Subscriber + Send + Sync) {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_span_processor(SimpleSpanProcessor::new(exporter.clone()))
            .build();
        let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "test");
        let subscriber = Registry::default()
            .with(tracing_opentelemetry::layer().with_tracer(tracer));
        (exporter, provider, subscriber)
    }
}

#[test]
fn test_coords_regex() {
    let false_cases = [
        "",
        "  ",
        ".,",
        "!",
        "123",
        "1,2",
        "1.2,3.4",
        "1,2,3,4",
    ];
    let true_cases = [
        "1 2",
        "1.2 3.4",
        "1,2 3,4",
        "1.2  3.4",
        "1,2  3,4",
        "1.2, 3.4",
        "12.345 67.89",
    ];

    run_test(false_cases, true_cases, |case| COORDS_REGEXP.is_match(case))
}

#[test]
fn test_query_regex() {
    let false_cases = [
        "",
        "  ",
        ".,",
        "!",
        "123",
        "1,2",
        "1.2,3.4",
        "1,2,3,4",
        "1 2",
        "1.2 3.4",
        "1,2 3,4",
        "1.2  3.4",
        "1,2  3,4",
        "1.2, 3.4",
        "12.345 67.89",
    ];
    let true_cases = [
        "Avenue",
        "Ave 12",
        "Kremlin, Moscow, Russia",
        "Кремль, Москва, Россия",
        "中国北京",
        "دبي مارينا، دبي، الإمارات العربية المتحدة",
    ];

    run_test(false_cases, true_cases, |case| QUERY_REGEX.is_match(case))
}

#[test]
fn test_is_query_correct() {
    let false_cases = [
        "",
        "  ",
        ".,",
        "!",
        "123",
        "1,2",
        "1.2,3.4",
        "1,2,3,4",
    ];
    let true_cases = [
        "Avenue",
        "Ave 12",
        "1 2",
        "1.2 3.4",
        "1,2 3,4",
        "1.2  3.4",
        "1.2, 3.4",
        "12.345 67.89",
        "Kremlin, Moscow, Russia",
        "Кремль, Москва, Россия",
        "中国北京",
        "دبي مارينا، دبي، الإمارات العربية المتحدة",
    ];

    run_test(false_cases, true_cases, is_query_correct)
}

#[tokio::test]
async fn test_resolve_locations_creates_span() {
    let (exporter, _provider, subscriber) = otel::setup_otel_test();
    let _guard = tracing::subscriber::set_default(subscriber);

    let result = super::resolve_locations("55.7 37.6".to_string(), "en", None).await;
    assert!(result.is_ok());

    let spans = exporter.get_finished_spans().unwrap();
    assert!(
        spans.iter().any(|s| s.name == "resolve_locations"),
        "Expected span 'resolve_locations', got: {:?}",
        spans.iter().map(|s| s.name.as_ref()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_span_hierarchy() {
    use tracing::Instrument;

    let (exporter, _provider, subscriber) = otel::setup_otel_test();
    let _guard = tracing::subscriber::set_default(subscriber);

    let root_span = tracing::info_span!("root");
    super::resolve_locations("55.7 37.6".to_string(), "en", None)
        .instrument(root_span)
        .await
        .unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    let root = spans.iter().find(|s| s.name == "root").expect("root span not found");
    let child = spans.iter().find(|s| s.name == "resolve_locations").expect("child span not found");

    assert_eq!(
        child.span_context.trace_id(),
        root.span_context.trace_id(),
        "child and root must share the same trace_id"
    );
    assert_eq!(
        child.parent_span_id,
        root.span_context.span_id(),
        "child's parent_span_id must equal root's span_id"
    );
}

fn run_test<const N1: usize, const N2: usize>(
    false_cases: [&str; N1],
    true_cases: [&str; N2],
    runner: fn(&str) -> bool
) {
    let false_cases = false_cases.into_iter().map(|case| (case, false));
    let true_cases  = true_cases.into_iter().map(|case| (case, true));

    for (param, expected) in false_cases.chain(true_cases) {
        assert_eq!(expected, runner(param), "param: '{param}'");
    }
}
