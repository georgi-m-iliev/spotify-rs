//! File-based logging module for Spotify-RS
//!
//! This module sets up tracing-based logging that writes to a file instead of stdout,
//! since the application uses a TUI that occupies the terminal.

use std::path::Path;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

const LOG_DIR: &str = ".logs";
const LOG_FILE_PREFIX: &str = "spotify-rs";

/// Initialize the logging system.
///
/// Logs are written to `.logs/spotify-rs.YYYY-MM-DD.log` with daily rotation.
/// The log level can be controlled via the `RUST_LOG` environment variable.
///
/// Default log levels:
/// - `spotify_rs` modules: DEBUG
/// - `librespot`: INFO
/// - Other crates: WARN
pub fn init_logging() -> anyhow::Result<()> {
    // Ensure log directory exists
    let log_dir = Path::new(LOG_DIR);
    if !log_dir.exists() {
        std::fs::create_dir_all(log_dir)?;
    }

    // Create a daily rotating file appender
    let file_appender = RollingFileAppender::new(Rotation::DAILY, LOG_DIR, LOG_FILE_PREFIX);

    // Create a non-blocking writer to avoid blocking the async runtime
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Keep the guard alive for the lifetime of the application
    // We use Box::leak to keep it alive without storing it
    Box::leak(Box::new(_guard));

    // Set up the filter from RUST_LOG env var, or use defaults
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
            "spotify_rs=debug,librespot=info,rspotify=info,warn"
        )
    });

    // Build the subscriber with file output only
    let fmt_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false) // No ANSI colors in log files
        .with_target(true) // Include module path
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_span_events(FmtSpan::CLOSE); // Log when spans close

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    tracing::info!("Logging initialized - logs written to {}/", LOG_DIR);

    Ok(())
}

/// Log a Spotify API request and its result
#[macro_export]
macro_rules! log_api_result {
    ($operation:expr, $result:expr) => {
        match &$result {
            Ok(_) => tracing::info!(operation = $operation, "API request successful"),
            Err(e) => tracing::error!(operation = $operation, error = %e, "API request failed"),
        }
    };
}

/// Log a Spotify API request with additional context
#[macro_export]
macro_rules! log_api_request {
    ($operation:expr, $($field:tt)*) => {
        tracing::debug!(operation = $operation, $($field)*, "API request started");
    };
}
