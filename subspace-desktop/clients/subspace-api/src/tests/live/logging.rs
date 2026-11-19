//! The stream request keeps the bearer token out of HTTP logs.
use super::{JWT, connect, server::Server, sse};
use std::sync::atomic::{AtomicBool, Ordering};

struct Records {
    leaked: AtomicBool,
}

impl log::Log for Records {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    /// The blocking transport's trace-level wire dump (`ureq_proto`) belongs
    /// to concurrent REST tests sharing this process-wide logger, not to the
    /// stream, whose HTTP client must never log the bearer.
    fn log(&self, record: &log::Record) {
        if !record.target().starts_with("ureq_proto") && record.args().to_string().contains(JWT) {
            self.leaked.store(true, Ordering::Relaxed);
        }
    }

    fn flush(&self) {}
}

static RECORDS: Records = Records {
    leaked: AtomicBool::new(false),
};

#[tokio::test]
async fn a_logger_never_sees_the_stream_assertion() {
    log::set_logger(&RECORDS).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    let server = Server::new(sse);
    let _stream = connect(&server).await.unwrap();
    assert_eq!(
        server.peer().seen.header("authorization"),
        Some(format!("Bearer {JWT}").as_str())
    );
    assert!(!RECORDS.leaked.load(Ordering::Relaxed));
}
