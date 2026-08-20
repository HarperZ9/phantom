use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub fn init(console: bool) {
    let log_dir = phantom_cli::profile::logs_dir();
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "phantom-svc.log");

    let filter = std::env::var("PHANTOM_LOG_LEVEL")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .and_then(|v| EnvFilter::try_new(v).ok())
        .unwrap_or_else(|| EnvFilter::new("info"));

    if console {
        let file_layer = fmt::layer()
            .with_writer(file_appender)
            .with_target(false)
            .with_ansi(false);

        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(false)
            .with_ansi(false);

        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_writer(file_appender)
                    .with_target(false)
                    .with_ansi(false),
            )
            .init();
    }
}
