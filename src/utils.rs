use std::{path::PathBuf, sync::LazyLock};

use time::{UtcOffset, format_description::well_known};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt::time::OffsetTime};

pub static LOCAL_OFFSET: LazyLock<UtcOffset> =
    LazyLock::new(|| match time::UtcOffset::current_local_offset() {
        Ok(offset) => offset,
        Err(e) => {
            tracing::error!("failed to get local offset: {}", e);
            time::UtcOffset::UTC
        }
    });

pub fn now_local() -> time::OffsetDateTime {
    // time::OffsetDateTime::now_local() is hard to use and has performance issue
    time::OffsetDateTime::now_utc().to_offset(*LOCAL_OFFSET)
}

/// sleep until the specified time
pub fn sleep_until(until: time::Time) {
    let now = now_local();
    let until = now.replace_time(until);
    let mut delta = until - now;
    if delta.is_negative() {
        delta = time::Duration::seconds(0);
    }
    std::thread::sleep(delta.unsigned_abs());
}

/// sleep until the next day at the specified time
pub fn sleep_until_next_day(until: time::Time) {
    let now = now_local();
    let until = now
        .replace_date(now.date().next_day().expect("unreachable"))
        .replace_time(until);
    std::thread::sleep((until - now).unsigned_abs());
}

/// initialize the log
pub fn init_log(log_dir: Option<PathBuf>) -> tracing_appender::non_blocking::WorkerGuard {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    let mut subscriber_builder = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_timer(OffsetTime::new(*LOCAL_OFFSET, well_known::Rfc3339));
    let (non_blocking, guard) = if let Some(log_dir) = log_dir {
        // output to file，daily rotate, non-blocking
        if !log_dir.is_dir() {
            panic!("log path is not a directory");
        }
        let file_appender = tracing_appender::rolling::daily(log_dir, "book_server.log");
        tracing_appender::non_blocking(file_appender)
    } else {
        subscriber_builder = subscriber_builder.with_ansi(true);
        // output to stderr
        tracing_appender::non_blocking(std::io::stderr())
    };
    let subscriber = subscriber_builder.with_writer(non_blocking).finish();
    tracing::subscriber::set_global_default(subscriber).expect("init log failed");
    guard
}
