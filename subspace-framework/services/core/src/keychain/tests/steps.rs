//! Saves and deletes that stop at any step, and operations that interleave.

use std::collections::BTreeSet;

use super::memory::{ACCOUNT, Memory, PIECE_UNITS, WINDOWS, assert_no_orphans};
use crate::keychain::KeychainError;
use crate::keychain::split::{delete, load, save};

const SHORT: &str = "fixture-short";

/// A secret of five chunks.
fn long(fill: char) -> String {
    fill.to_string().repeat(4 * PIECE_UNITS + 1)
}

fn transitions() -> [(String, String); 4] {
    [
        (SHORT.to_owned(), "fixture-next".to_owned()),
        (SHORT.to_owned(), long('n')),
        (long('p'), SHORT.to_owned()),
        (long('p'), long('n')),
    ]
}

/// The steps an operation takes on a store holding `previous`.
fn steps(previous: &str, operation: impl FnOnce(&Memory)) -> usize {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, previous).unwrap();
    store.fail_step(usize::MAX, false);
    operation(&store);
    store.steps()
}

#[test]
fn a_save_that_stops_at_any_step_keeps_one_whole_secret() {
    for (previous, next) in transitions() {
        let total = steps(&previous, |store| {
            save(store, WINDOWS, ACCOUNT, &next).unwrap()
        });
        for (step, crash) in (1..=total).flat_map(|step| [(step, false), (step, true)]) {
            let store = Memory::windows();
            save(&store, WINDOWS, ACCOUNT, &previous).unwrap();
            let before = store.items();
            store.fail_step(step, crash);
            let saved = save(&store, WINDOWS, ACCOUNT, &next);
            store.recover();
            let expected = if saved.is_ok() { &next } else { &previous };
            assert_eq!(
                load(&store, WINDOWS, ACCOUNT).as_ref(),
                Ok(&Some(expected.clone()))
            );
            if saved.is_err() && !crash {
                // A save that fails removes the chunks it wrote.
                assert_eq!(store.items(), before);
            }
            save(&store, WINDOWS, ACCOUNT, &next).unwrap();
            assert_no_orphans(&store, ACCOUNT);
        }
    }
}

#[test]
fn a_delete_that_stops_at_any_step_leaves_the_secret_or_none() {
    for secret in [SHORT.to_owned(), long('d')] {
        let total = steps(&secret, |store| delete(store, WINDOWS, ACCOUNT).unwrap());
        for (step, crash) in (1..=total).flat_map(|step| [(step, false), (step, true)]) {
            let store = Memory::windows();
            save(&store, WINDOWS, ACCOUNT, &secret).unwrap();
            store.fail_step(step, crash);
            let deleted = delete(&store, WINDOWS, ACCOUNT);
            store.recover();
            let loaded = load(&store, WINDOWS, ACCOUNT).unwrap();
            assert!(loaded.is_none() || (deleted.is_err() && loaded == Some(secret.clone())));
            delete(&store, WINDOWS, ACCOUNT).unwrap();
            assert!(store.names().is_empty());
        }
    }
}

#[test]
fn a_crash_after_the_manifest_leaves_the_new_secret_and_a_later_sweep() {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, &long('p')).unwrap();
    let previous: BTreeSet<String> = store.names();
    // Five chunks, the manifest, then the sweep's first step.
    store.fail_step(7, true);
    assert_eq!(save(&store, WINDOWS, ACCOUNT, &long('n')), Ok(()));
    store.recover();
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(long('n'))));
    let names = store.names();
    assert!(previous.iter().all(|name| names.contains(name)));
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    assert!(store.names().is_empty());
}

#[test]
fn operations_interleaved_at_any_step_never_mix_secrets() {
    let first = long('a');
    let mut outcomes = BTreeSet::new();
    for other in [Some(long('b')), Some(SHORT.to_owned()), None] {
        let total = steps("fixture-old", |store| {
            save(store, WINDOWS, ACCOUNT, &first).unwrap()
        });
        for step in 1..=total {
            let store = Memory::windows();
            save(&store, WINDOWS, ACCOUNT, "fixture-old").unwrap();
            let interloper = other.clone();
            store.before_step(step, move |store| {
                let _ = match &interloper {
                    Some(secret) => save(store, WINDOWS, ACCOUNT, secret),
                    None => delete(store, WINDOWS, ACCOUNT),
                };
            });
            let _ = save(&store, WINDOWS, ACCOUNT, &first);
            let loaded = load(&store, WINDOWS, ACCOUNT);
            let allowed = [
                Ok(Some(first.clone())),
                Ok(other.clone()),
                Err(KeychainError),
            ];
            assert!(allowed.contains(&loaded));
            outcomes.insert(format!("{:?}", loaded.map(|value| value.map(|v| v.len()))));
            save(&store, WINDOWS, ACCOUNT, "fixture-after").unwrap();
            assert_no_orphans(&store, ACCOUNT);
        }
    }
    // Some interleavings sweep the chunks of a save not yet published.
    assert!(outcomes.contains("Err(KeychainError)"));
}
