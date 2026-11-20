//! Reserve below the shared edge budget, across readers and comment workers.
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};
const WINDOW: Duration = Duration::from_secs(10);
const REQUESTS: usize = 45;
static STARTS: Mutex<VecDeque<Instant>> = Mutex::new(VecDeque::new());

pub(crate) fn wait(url: &url::Url) {
    while let Some(delay) = request_delay(url) {
        std::thread::sleep(delay);
    }
}

pub(crate) async fn wait_async(url: &url::Url) {
    while let Some(delay) = request_delay(url) {
        tokio::time::sleep(delay).await;
    }
}

fn request_delay(url: &url::Url) -> Option<Duration> {
    if url.host_str().is_some_and(|host| {
        host.trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    }) {
        return None;
    }
    // NETT has no fixed window: its client limits concurrency and backs off
    // when refused, in `nett::flow`.
    if crate::backend() == crate::Backend::Nett {
        return None;
    }
    reserve(
        &mut STARTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        Instant::now(),
        REQUESTS,
    )
}

fn reserve(starts: &mut VecDeque<Instant>, now: Instant, budget: usize) -> Option<Duration> {
    while starts
        .front()
        .is_some_and(|at| now.saturating_duration_since(*at) >= WINDOW)
    {
        starts.pop_front();
    }
    if starts.len() >= budget {
        starts
            .front()
            .map(|at| WINDOW.saturating_sub(now.saturating_duration_since(*at)))
    } else {
        starts.push_back(now);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{REQUESTS, WINDOW, reserve};
    use std::{collections::VecDeque, time::Instant};
    #[test]
    fn budget_covers_bursts_and_releases_only_after_window() {
        let now = Instant::now();
        let mut starts = VecDeque::new();
        for _ in 0..REQUESTS {
            assert_eq!(reserve(&mut starts, now, REQUESTS), None);
        }
        assert_eq!(reserve(&mut starts, now, REQUESTS), Some(WINDOW));
        assert_eq!(reserve(&mut starts, now + WINDOW, REQUESTS), None);
    }
}
