use std::path::PathBuf;

/// 获取当前时间
pub fn local_now() -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc().to_offset(time::macros::offset!(+8))
}

/// 睡眠到当日指定时间点，如果时间超过，则立即执行
pub fn sleep_until(until: time::Time) {
    let now = local_now();
    let until = now.replace_time(until);
    let mut delta = until - now;
    if delta.is_negative() {
        delta = time::Duration::seconds(0);
    }
    std::thread::sleep(delta.unsigned_abs());
}

/// 睡眠到次日指定时间
pub fn sleep_until_next_day(until: time::Time) {
    let now = local_now();
    let until = now
        .replace_date(now.date().next_day().expect("unreachable"))
        .replace_time(until);
    std::thread::sleep((until - now).unsigned_abs());
}

/// 初始化日志
pub fn init_log(log: Option<PathBuf>) -> tracing_appender::non_blocking::WorkerGuard {
    let subscriber_builder = tracing_subscriber::fmt::Subscriber::builder()
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true);
    let (non_blocking, guard) = if let Some(log) = log {
        // output to file，daily rotate, non-blocking
        if !log.is_dir() {
            panic!("log path is not a directory");
        }
        let file_appender = tracing_appender::rolling::daily(log, "book_server.log");
        tracing_appender::non_blocking(file_appender)
    } else {
        // output to stdout
        tracing_appender::non_blocking(std::io::stdout())
    };
    tracing::subscriber::set_global_default(
        subscriber_builder.with_writer(non_blocking).finish(),
    )
    .expect("init log failed");
    guard
}
