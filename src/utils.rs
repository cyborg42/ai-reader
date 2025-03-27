use std::{path::PathBuf, sync::LazyLock};

use time::UtcOffset;

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
    let mut subscriber_builder = tracing_subscriber::fmt::Subscriber::builder()
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true);
    let (non_blocking, guard) = if let Some(log_dir) = log_dir {
        // output to file，daily rotate, non-blocking
        if !log_dir.is_dir() {
            panic!("log path is not a directory");
        }
        let file_appender = tracing_appender::rolling::daily(log_dir, "book_server.log");
        tracing_appender::non_blocking(file_appender)
    } else {
        subscriber_builder = subscriber_builder.with_ansi(true);
        // output to stdout
        tracing_appender::non_blocking(std::io::stdout())
    };
    let subscriber = subscriber_builder.with_writer(non_blocking).finish();
    tracing::subscriber::set_global_default(subscriber).expect("init log failed");
    guard
}
