mod service;
#[cfg(test)]
mod service_tests;

use std::error::Error;
use std::sync::Arc;

use messaging_model::s2sm::ServiceManager;

#[tokio::main]
async fn main() {
    // The chassis logger, NOT tracing_subscriber::fmt().
    //
    // tracing allows exactly ONE global subscriber per process, so a service that
    // builds its own wins permanently and the chassis's redacting install can
    // only fail. install_bootstrap redacts from the first line and leaves the
    // layer swappable so the typed logger config still applies later.
    //
    // It matters here for the same reason it matters in scotty, one layer down:
    // every row this service handles is one person's text addressed to another
    // person. An unredacted log line is the one place that leaves the
    // entitlement model behind — which is why nothing in service.rs logs a
    // subject, a body or a recipient list, only ids and counts.
    if let Err(e) = logger::install_bootstrap() {
        eprintln!("FATAL: could not install the redacting logger: {e}");
        std::process::exit(1);
    }

    if let Err(e) = run().await {
        tracing::error!(error = %e, "messaging failed");
        let mut source = e.source();
        while let Some(cause) = source {
            tracing::error!(cause = %cause, "caused by");
            source = cause.source();
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    // ServiceManager::new() reads BOOTSTRAP_CONFIG, connects to kernel over
    // mTLS, and returns once the service has an identity and config. sm.run()
    // then serves the generated gRPC surface and blocks until shutdown.
    let sm = Arc::new(ServiceManager::new().await?);

    // The topic is SYNTHESIZED from Message's spec.record, so its publisher
    // factory is named after the generated wrapper (MessageBus) and its field
    // (stream), not after anything hand-written. This is the ONLY path by which
    // a message leaves this service, which is why it is acquired before the
    // server starts rather than lazily on first use.
    //
    // Note what is NOT here: no eventbus client is constructed, no topic name is
    // typed out, no token is acquired for the "eventbus" audience. All four are
    // inside this generated closure. Hand-rolling an EventBus client would mean
    // rewriting every one of them.
    let publish = sm.create_messaging_message_bus_stream_publisher().await;

    let svc = Arc::new(service::MessagingService::new(publish));
    let handler = Arc::new(service::MessagingServiceHandler::new(svc));

    sm.run(handler).await
}
