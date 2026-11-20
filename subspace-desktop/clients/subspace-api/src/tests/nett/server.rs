//! Starts the loopback NETT mock for one test and stops it afterwards.

use serde_json::Value;
use std::io::{BufRead as _, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub(in crate::tests) struct NettMock {
    child: Child,
    origin: String,
}

impl NettMock {
    /// The running mock. Python 3 is already a prerequisite of the checks,
    /// so a missing interpreter fails the test rather than skipping it.
    pub(super) fn start() -> Self {
        Self::serving(&[])
    }

    /// The mock, advertising and serving these coming desktop features.
    pub(super) fn serving(features: &[&str]) -> Self {
        Self::with_args(features, &[])
    }

    /// The mock serving `features`, with further command-line options.
    pub(super) fn with_args(features: &[&str], extra: &[&str]) -> Self {
        let script =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/nett_api_test_server.py");
        let child = Command::new(
            subspace_framework::core::path::find_tool("python3")
                .expect("python3 runs the NETT mock"),
        )
        .arg("-I")
        .arg(&script)
        .args(["--port", "0", "--auto-approve"])
        .args(features.iter().flat_map(|feature| ["--serve", feature]))
        .args(extra)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
        let mut child = child.expect("python3 runs the NETT mock");
        let mut line = String::new();
        if let Some(stdout) = child.stdout.take() {
            let _ = BufReader::new(stdout).read_line(&mut line);
        }
        let origin = line
            .trim()
            .strip_prefix("NETT fixture listening on ")
            .map(str::to_owned);
        let Some(origin) = origin else {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the NETT mock did not start: {line:?}");
        };
        Self { child, origin }
    }

    pub(super) fn api(&self) -> String {
        format!("{}/api", self.origin)
    }

    fn hook(&self, path: &str) -> String {
        format!("{}/__fixture/{path}", self.origin)
    }

    pub(super) fn requests(&self) -> Vec<Value> {
        let mut response = ureq::get(&self.hook("requests")).call().unwrap();
        let text = response.body_mut().read_to_string().unwrap();
        serde_json::from_str(&text).unwrap()
    }

    pub(super) fn clear_requests(&self) {
        ureq::post(&self.hook("requests/clear"))
            .send_empty()
            .unwrap();
    }

    /// Any fixture hook, by path.
    pub(super) fn post_hook(&self, path: &str, body: &Value) {
        self.post(path, body);
    }

    fn post(&self, path: &str, body: &Value) {
        ureq::post(&self.hook(path))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .unwrap();
    }

    /// Sets fixture behaviour, such as completions that check first or a
    /// refused update stream.
    pub(super) fn config(&self, body: &Value) {
        self.post("config", body);
    }

    /// Replaces one organisation's capability list.
    pub(super) fn capabilities(&self, org: &str, features: &[&str]) {
        self.post(
            "capabilities",
            &serde_json::json!({"org": org, "features": features}),
        );
    }

    /// Another bearer acting as the caller, refused every file grant.
    pub(super) fn grantless_token(&self, token: &str) {
        self.post(
            "tokens",
            &serde_json::json!({"token": token, "denyGrants": true}),
        );
    }

    /// Another bearer acting as `user` (ada, grace or linus).
    pub(super) fn token(&self, token: &str, user: &str) {
        self.post(
            "tokens",
            &serde_json::json!({"token": token, "user": user, "denyGrants": false}),
        );
    }

    /// Refuses `token` from now on, closing its streams.
    pub(super) fn revoke_token(&self, token: &str) {
        self.post("tokens/revoke", &serde_json::json!({"token": token}));
    }

    /// Ends `user`'s membership of `org`. Open streams close at their next
    /// flush, without sending what was queued.
    pub(super) fn revoke_member(&self, org: &str, user: &str) {
        self.post(
            "members/revoke",
            &serde_json::json!({"org": org, "user": user, "closeStreams": false}),
        );
    }

    /// The next `times` requests to `path` with `method` get `status`.
    pub(super) fn fail_next(
        &self,
        method: &str,
        path: &str,
        status: u16,
        retry_after: Option<u64>,
        times: u32,
    ) {
        let mut rule =
            serde_json::json!({"method": method, "path": path, "status": status, "times": times});
        if let Some(seconds) = retry_after {
            rule["retryAfter"] = serde_json::json!(seconds);
        }
        self.config(&serde_json::json!({"failNext": [rule]}));
    }

    /// Closes open update streams, as on overflow or lost authority.
    pub(super) fn close_streams(&self) {
        self.post("updates/close", &serde_json::json!({}));
    }

    pub(super) fn archive_team(&self, org: &str, key: &str) {
        self.post(
            "teams/archive",
            &serde_json::json!({"org": org, "key": key}),
        );
    }

    pub(super) fn archive_issue(&self, org: &str, issue: &str) {
        self.post(
            "issues/archive",
            &serde_json::json!({"org": org, "issue": issue}),
        );
    }

    /// Logged requests whose path ends with `suffix`, made with `method`.
    pub(super) fn count(&self, method: &str, suffix: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| {
                request["method"] == method
                    && request["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with(suffix))
            })
            .count()
    }

    /// A change made elsewhere: the issue's activity time moves on.
    pub(super) fn touch(&self, org: &str, issue: &str) {
        ureq::post(&self.hook("touch"))
            .header("Content-Type", "application/json")
            .send(serde_json::json!({"org": org, "issue": issue}).to_string())
            .unwrap();
    }
}

impl Drop for NettMock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
