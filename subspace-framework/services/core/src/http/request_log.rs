//! One log record per request through the shared agent: the method, the
//! host, the path without its query string, the status and the duration.
//! Never a header, a query string or a body.

use std::time::Instant;

use ureq::middleware::{Middleware, MiddlewareNext};
use ureq::{Body, SendBody};

use crate::http::status::BAD_REQUEST;
use crate::log::{Level, Logger};

/// The log target every client request is filed under.
pub const TARGET: &str = "http";

pub(super) struct RequestLog;

impl Middleware for RequestLog {
    fn handle(
        &self,
        request: ureq::http::Request<SendBody>,
        next: MiddlewareNext,
    ) -> Result<ureq::http::Response<Body>, ureq::Error> {
        let method = request.method().to_string();
        let host = request.uri().host().unwrap_or_default().to_owned();
        let path = request.uri().path().to_owned();
        let started = Instant::now();
        let result = next.handle(request);
        let outcome = match &result {
            Ok(response) => Outcome::Status(response.status().as_u16()),
            Err(error) => Outcome::Failed(error.to_string()),
        };
        let line = Line {
            method,
            host,
            path,
            outcome,
            millis: started.elapsed().as_millis(),
        };
        log(&line);
        result
    }
}

/// How a request ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Status(u16),
    Failed(String),
}

/// What one request's record says.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    method: String,
    host: String,
    path: String,
    outcome: Outcome,
    millis: u128,
}

impl Line {
    /// Debug for a response under 400, Warn for a failure status or no
    /// response at all.
    fn level(&self) -> Level {
        match &self.outcome {
            Outcome::Status(status) if *status < BAD_REQUEST => Level::Debug,
            _ => Level::Warn,
        }
    }

    /// `GET management.azure.com /subscriptions/x 200 843ms`.
    fn message(&self) -> String {
        let ending = match &self.outcome {
            Outcome::Status(status) => status.to_string(),
            Outcome::Failed(error) => format!("failed: {error}"),
        };
        format!(
            "{} {} {} {ending} {}ms",
            self.method, self.host, self.path, self.millis
        )
    }

    fn status_text(&self) -> String {
        match &self.outcome {
            Outcome::Status(status) => status.to_string(),
            Outcome::Failed(_) => "0".to_owned(),
        }
    }
}

fn log(line: &Line) {
    let millis = line.millis.to_string();
    let status = line.status_text();
    Logger::new(TARGET).record(
        line.level(),
        line.message(),
        &[
            ("method", &line.method),
            ("host", &line.host),
            ("path", &line.path),
            ("status", &status),
            ("duration_ms", &millis),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::{Line, Outcome};
    use crate::log::Level;

    fn line(outcome: Outcome) -> Line {
        Line {
            method: "POST".into(),
            host: "management.azure.com".into(),
            path: "/subscriptions/x/providers/Microsoft.CostManagement/query".into(),
            outcome,
            millis: 843,
        }
    }

    #[test]
    fn a_request_line_names_the_method_host_path_status_and_time() {
        let ok = line(Outcome::Status(200));
        assert_eq!(
            ok.message(),
            "POST management.azure.com /subscriptions/x/providers/Microsoft.CostManagement/query 200 843ms"
        );
        assert_eq!(ok.level(), Level::Debug);
        assert_eq!(ok.status_text(), "200");
        let limited = line(Outcome::Status(429));
        assert_eq!(limited.level(), Level::Warn);
        let failed = line(Outcome::Failed("connection refused".into()));
        assert_eq!(failed.level(), Level::Warn);
        assert!(
            failed
                .message()
                .ends_with("failed: connection refused 843ms")
        );
        assert_eq!(failed.status_text(), "0");
    }
}
