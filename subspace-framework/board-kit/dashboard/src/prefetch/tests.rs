use super::{
    Cancellation, Config, Failure, Source,
    schedule::{Next, Schedule},
};
use std::time::{Duration, Instant};
use subspace_search::{Policy, Scope, Service};

#[test]
fn scheduler_bounds_loaded_items_concurrency_and_retries() {
    let now = Instant::now();
    let config = Config {
        concurrency: 2,
        max_items: 3,
        debounce: Duration::ZERO,
    };
    let mut schedule = Schedule::new(vec![1, 2, 3, 4], config, now);
    assert!(matches!(schedule.next(now), Next::Item(1, 0)));
    assert!(matches!(schedule.next(now), Next::Item(2, 0)));
    assert!(matches!(schedule.next(now), Next::Wait(_)));
    schedule.complete(1, 0, Err(Failure::RateLimit), now);
    schedule.complete(2, 0, Ok(()), now);
    assert!(
        matches!(schedule.next(now), Next::Wait(_)),
        "success must not bypass a concurrent throttle"
    );
    let later = now + Duration::from_secs(600);
    assert!(matches!(schedule.next(later), Next::Item(1, 1)));
    schedule.complete(1, 1, Ok(()), later);
    assert!(matches!(schedule.next(later), Next::Item(3, 0)));
    schedule.complete(3, 0, Ok(()), later);
    assert!(matches!(schedule.next(later), Next::Done));
}

#[test]
fn debounce_and_retry_budget_bound_a_failing_item() {
    let mut now = Instant::now();
    let mut schedule = Schedule::new(vec![1], Config::default(), now);
    assert!(matches!(schedule.next(now), Next::Wait(_)));
    for attempt in 0..=2 {
        now += Duration::from_secs(600);
        assert!(matches!(schedule.next(now), Next::Item(1, retry) if retry == attempt));
        schedule.complete(1, attempt, Err(Failure::Transient), now);
    }
    assert!(matches!(
        schedule.next(now + Duration::from_secs(600)),
        Next::Done
    ));
}

#[test]
fn authentication_stops_and_cancellation_is_shared_across_workers() {
    let now = Instant::now();
    let mut schedule = Schedule::new(
        vec![1, 2],
        Config {
            debounce: Duration::ZERO,
            ..Config::default()
        },
        now,
    );
    assert!(matches!(schedule.next(now), Next::Item(1, 0)));
    schedule.complete(1, 0, Err(Failure::Authentication), now);
    assert!(matches!(
        schedule.next(now + Duration::from_secs(3600)),
        Next::Done
    ));
    let cancellation = Cancellation::default();
    let worker_token = cancellation.clone();
    worker_token.cancel();
    assert!(cancellation.cancelled());
    assert_eq!(
        Failure::from(&subspace_core::http::Error::Status {
            status: 429,
            message: String::new()
        }),
        Failure::RateLimit
    );
}

#[test]
fn source_requires_opt_in_and_captured_lease_fails_after_disable() {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(
        root.path().join("search"),
        "app".into(),
        vec!["board".into()],
    )
    .unwrap();
    assert!(
        Source::bind(service.clone(), "board", || panic!("disabled binding"))
            .unwrap()
            .is_none()
    );
    let source = enabled_source(service.clone());
    let work = source.context(Cancellation::default()).unwrap();
    service.set_policy("board", Policy::default()).unwrap();
    assert!(work.publish(&[]).is_err());
    assert!(source.context(Cancellation::default()).is_err());
    let rebound = enabled_source(service);
    assert!(rebound.context(Cancellation::default()).is_ok());
    assert!(
        source.context(Cancellation::default()).is_err(),
        "a new run must not revive the old run after disable/re-enable"
    );
}

fn enabled_source(service: Service) -> Source {
    service
        .set_policy(
            "board",
            Policy {
                enabled: true,
                ..Policy::default()
            },
        )
        .unwrap();
    Source::bind(service, "board", || {
        Ok(Scope {
            app: "app".into(),
            board: "board".into(),
            endpoint: "https://example.test".into(),
            tenant: "tenant".into(),
            account: "verified".into(),
        })
    })
    .unwrap()
    .unwrap()
}
