#![deny(unused_crate_dependencies)]

use axum::routing::get;
use axum::Router;
use opentelemetry::sdk::{trace as sdktrace, Resource};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{ExportConfig, Protocol, WithExportConfig};
use reqwest as _; // Need to pin version of reqwest to avoid "error trying to connect: invalid URL, scheme is not http"
use std::collections::HashMap;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{span, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// Expecting a config/.honeycomb_api_key file with a single line that is the Honeycomb API key
const HONEYCOMB_API_KEY: &str = include_str!("../config/.honeycomb_api_key");

#[tokio::main]
async fn main() {
    let tracer = init_tracer();

    let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    tracing_subscriber::registry()
        .with(opentelemetry)
        .try_init()
        .unwrap();

    let app = Router::new()
        .route("/", get(handler))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

fn init_tracer() -> sdktrace::Tracer {
    let metadata = HashMap::from([(
        "x-honeycomb-team".to_string(),
        HONEYCOMB_API_KEY.to_string(),
    )]);

    let export_config = ExportConfig {
        endpoint: "https://api.honeycomb.io/v1/traces".to_string(),
        timeout: Duration::from_secs(3),
        protocol: Protocol::HttpBinary,
    };

    let trace_config =
        opentelemetry::sdk::trace::config().with_resource(Resource::new(vec![KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            "Pick List",
        )]));

    let otlp_exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_headers(metadata)
        .with_export_config(export_config);

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(otlp_exporter)
        .with_trace_config(trace_config)
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap();

    tracer
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::warn!("signal received, starting graceful shutdown");
    opentelemetry::global::shutdown_tracer_provider();
}

async fn handler() -> &'static str {
    span!(Level::INFO, "my_span").in_scope(|| "Hello, world!")
}
