use crate::paths::AppPaths;
use std::io::IsTerminal;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init() -> Option<WorkerGuard> {
    let stderr_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_ansi(std::io::stderr().is_terminal())
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_writer(std::io::stderr);

    match file_writer() {
        Some((writer, guard)) => {
            let file_layer = tracing_subscriber::fmt::layer()
                .compact()
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(writer);
            let result = tracing_subscriber::registry()
                .with(default_filter())
                .with(stderr_layer)
                .with(file_layer)
                .try_init();
            if result.is_ok() {
                Some(guard)
            } else {
                None
            }
        }
        None => {
            let _ = tracing_subscriber::registry()
                .with(default_filter())
                .with(stderr_layer)
                .try_init();
            None
        }
    }
}

fn default_filter() -> EnvFilter {
    let app_filter = if cfg!(debug_assertions) {
        "hearthstone_linux=trace,CoreFoundation=trace,OSXWindowManagement=trace,blz_commerce_sdk_plugin=trace"
    } else {
        "hearthstone_linux=info,CoreFoundation=info,OSXWindowManagement=info,blz_commerce_sdk_plugin=info"
    };

    match std::env::var("RUST_LOG") {
        Ok(filter) if mentions_app_filter(&filter) => EnvFilter::new(filter),
        Ok(filter) if !filter.trim().is_empty() => EnvFilter::new(format!("{filter},{app_filter}")),
        _ if cfg!(debug_assertions) => EnvFilter::new(format!("info,{app_filter}")),
        _ => EnvFilter::new(format!("info,{app_filter}")),
    }
}

fn mentions_app_filter(filter: &str) -> bool {
    [
        "hearthstone_linux",
        "CoreFoundation",
        "OSXWindowManagement",
        "blz_commerce_sdk_plugin",
    ]
    .iter()
    .any(|target| filter.contains(target))
}

fn file_writer() -> Option<(NonBlocking, WorkerGuard)> {
    let paths = AppPaths::discover().ok()?;
    std::fs::create_dir_all(&paths.log_dir).ok()?;
    let file = tracing_appender::rolling::daily(paths.log_dir, "hearthstone-linux-gui.log");
    Some(tracing_appender::non_blocking(file))
}
