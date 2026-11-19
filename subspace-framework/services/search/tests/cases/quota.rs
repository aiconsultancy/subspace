use super::{document, enable, query, setup};
use subspace_search::Policy;

const WINDOWS_ALLOCATION_UNIT: u64 = 65_536;

#[test]
fn minimum_and_default_quotas_admit_a_fresh_windows_store_write() {
    let (root, service, scope) = setup();
    let path = root.path().join("search/617070/697263.sqlite");
    let connection = rusqlite::Connection::open(path).unwrap();
    let page_size = connection
        .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
        .unwrap() as u64;
    let page_count = connection
        .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
        .unwrap() as u64;
    let database_bytes = page_size * page_count;
    let windows_bytes = database_bytes.div_ceil(WINDOWS_ALLOCATION_UNIT) * WINDOWS_ALLOCATION_UNIT;
    let required_quota = windows_bytes * 3 + WINDOWS_ALLOCATION_UNIT;
    assert_eq!(
        (page_count, database_bytes, windows_bytes),
        (24, 98_304, 131_072)
    );
    assert_eq!(Policy::MIN_MAX_BYTES, required_quota);
    assert!(Policy::default().max_bytes >= required_quota);
    drop(connection);

    assert!(
        service
            .set_policy(
                "irc",
                Policy {
                    max_bytes: Policy::MIN_MAX_BYTES - 1,
                    ..Policy::default()
                },
            )
            .is_err()
    );
    service
        .set_policy(
            "irc",
            Policy {
                enabled: true,
                max_bytes: Policy::MIN_MAX_BYTES,
                ..Policy::default()
            },
        )
        .unwrap();
    service.activate(&scope).unwrap();
    service
        .upsert(&scope, &[document("minimum", "fresh write")])
        .unwrap();
    assert_eq!(service.search(&query("fresh")).unwrap().hits.len(), 1);

    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service
        .upsert(&scope, &[document("default", "default write")])
        .unwrap();
    assert_eq!(service.search(&query("default")).unwrap().hits.len(), 1);
}

#[test]
fn full_store_evicts_oldest_and_admits_new_content() {
    const QUOTA: u64 = 1_048_576;
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    service
        .set_policy(
            "irc",
            Policy {
                enabled: true,
                max_bytes: QUOTA,
                ..Policy::default()
            },
        )
        .unwrap();
    let body = "retained searchable ".repeat(800);
    for index in 0..30 {
        let mut value = document(&index.to_string(), &body);
        value.refreshed_at -= 100 - index;
        service.upsert(&scope, &[value]).unwrap_or_else(|error| {
            panic!(
                "write {index}: {error}; quota={QUOTA}; source status={:?}",
                service.sources()
            )
        });
    }
    let hits = service.search(&query("retained")).unwrap().hits;
    assert!(hits.iter().any(|hit| hit.document.id == "29"));
    assert!(!hits.iter().any(|hit| hit.document.id == "0"));
    assert!(service.sources().unwrap()[0].bytes <= QUOTA);
}

#[test]
fn allocated_overhead_counts_once_towards_the_write_quota() {
    const QUOTA: u64 = 640 * 1024;
    let (root, service, scope) = setup();
    enable(&service, &scope);
    service
        .set_policy(
            "irc",
            Policy {
                enabled: true,
                max_bytes: QUOTA,
                ..Policy::default()
            },
        )
        .unwrap();

    // In DELETE journal mode SQLite leaves this sidecar alone. It supplies
    // deterministic extra allocation without depending on filesystem preallocation.
    let sidecar = root.path().join("search/617070/697263.sqlite-shm");
    std::fs::write(&sidecar, vec![0u8; 128 * 1024]).unwrap();
    service
        .upsert(&scope, &[document("within-budget", "admitted content")])
        .unwrap();
    assert_eq!(service.search(&query("admitted")).unwrap().hits.len(), 1);
    assert!(service.sources().unwrap()[0].bytes <= QUOTA);

    std::fs::write(&sidecar, vec![0u8; 768 * 1024]).unwrap();
    assert!(
        service
            .upsert(&scope, &[document("over-budget", "rejected content")])
            .is_err()
    );
    assert_eq!(service.search(&query("admitted")).unwrap().hits.len(), 1);
    assert!(service.search(&query("rejected")).unwrap().hits.is_empty());
}

#[test]
fn index_growth_hits_page_limit_and_rolls_back_content_and_fts_together() {
    let (_root, service, scope) = setup();
    enable(&service, &scope);
    let quota = 500_000;
    service
        .set_policy(
            "irc",
            Policy {
                enabled: true,
                max_bytes: quota,
                ..Policy::default()
            },
        )
        .unwrap();
    let body = (0..8000)
        .map(|index| format!("word{index} "))
        .collect::<String>();
    assert!(body.len() < 128 * 1024);
    assert!(
        service
            .upsert(&scope, &[document("oversize-index", &body)])
            .is_err()
    );
    assert_eq!(service.sources().unwrap()[0].records, 0);
    assert!(service.sources().unwrap()[0].bytes <= quota);
    assert!(service.search(&query("word1")).unwrap().hits.is_empty());
    service
        .upsert(&scope, &[document("small", "recovered")])
        .unwrap();
    assert_eq!(service.search(&query("recovered")).unwrap().hits.len(), 1);
    assert!(
        service
            .upsert(&scope, &[document("still-too-large", &body)])
            .is_err()
    );
    assert_eq!(
        service.search(&query("recovered")).unwrap().hits.len(),
        1,
        "failed quota retries must roll back eviction with the write"
    );
}

#[cfg(unix)]
#[test]
fn database_and_app_directory_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (root, _service, _scope) = setup();
    let directory = root.path().join("search").join("617070");
    assert_eq!(
        std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let file = directory.join("697263.sqlite");
    assert_eq!(
        std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
