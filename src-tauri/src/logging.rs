use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Initialise tracing / logging.
///
/// * Writes file logs with daily rotation into the application data directory.
/// * Also prints to stdout (helpful during development).
///
/// Returns the `WorkerGuard` for the file appender – the caller **must**
/// keep this value alive for the lifetime of the application or the file
/// writer thread will be shut down and logs will be lost.
pub fn setup_logging(log_level: &str) -> WorkerGuard {
    let log_dir = crate::config::log_dir();
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        log_dir,
        "home-net.log",
    );

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    let fmt_layer_stdout = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(false)
        .with_writer(std::io::stdout);

    let fmt_layer_file = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(false)
        .with_writer(non_blocking)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer_stdout)
        .with(fmt_layer_file)
        .init();

    // The guard must be kept alive – return it so the caller (lib.rs) holds it.
    guard
}
