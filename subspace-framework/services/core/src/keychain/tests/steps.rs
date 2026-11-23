//! Saves and deletes that stop at any step, and operations that interleave.

use std::collections::BTreeSet;

use super::expect::assert_no_orphans;
use super::memory::{ACCOUNT, Failure, Memory, PIECE_UNITS, WINDOWS};
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
    store.fail_step(usize::MAX, Failure::Error);
    operation(&store);
    store.steps()
}

/// A store holding `previous` whose step `step` of the next operation fails.
fn failing(previous: &str, step: usize, failure: Failure) -> Memory {
    let store = Memory::windows();
    save(&store, WINDOWS, ACCOUNT, previous).unwrap();
    store.fail_step(step, failure);
    store
}

#[test]
fn a_save_that_stops_at_any_step_keeps_one_whole_secret() {
    let failures = [Failure::Error, Failure::Crash, Failure::Lands];
    for (previous, next) in transitions() {
        let total = steps(&previous, |store| {
            save(store, WINDOWS, ACCOUNT, &next).unwrap()
        });
        let cases = (1..=total).flat_map(|step| failures.map(|failure| (step, failure)));
        for (step, failure) in cases {
            let store = failing(&previous, step, failure);
            let before = store.items();
            let saved = save(&store, WINDOWS, ACCOUNT, &next);
            store.recover();
            let loaded = load(&store, WINDOWS, ACCOUNT).unwrap();
            assert!(loaded == Some(previous.clone()) || loaded == Some(next.clone()));
            // A save that succeeds holds the new secret; one that fails
            // cleanly changes nothing.
            assert!(saved.is_err() || loaded.as_ref() == Some(&next));
            let clean = saved.is_ok() || !matches!(failure, Failure::Error);
            assert!(clean || store.items() == before);
            delete(&store, WINDOWS, ACCOUNT).unwrap();
            assert!(store.names().is_empty());
        }
    }
}

#[test]
fn a_commit_that_lands_but_reports_a_failure_keeps_its_chunks() {
    // The previous item is read, five chunks written, then the commit.
    let store = failing(SHORT, 7, Failure::Lands);
    assert_eq!(
        save(&store, WINDOWS, ACCOUNT, &long('n')),
        Err(KeychainError)
    );
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(long('n'))));
    assert_no_orphans(&store, ACCOUNT);

    // When the item cannot be read again, the chunks stay too.
    let store = failing(SHORT, 7, Failure::Lands);
    store.also_fail(8, Failure::Error);
    assert_eq!(
        save(&store, WINDOWS, ACCOUNT, &long('n')),
        Err(KeychainError)
    );
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(long('n'))));
    assert_no_orphans(&store, ACCOUNT);
}

#[test]
fn a_delete_that_stops_at_any_step_leaves_the_secret_or_none() {
    for secret in [SHORT.to_owned(), long('d')] {
        let total = steps(&secret, |store| delete(store, WINDOWS, ACCOUNT).unwrap());
        for step in 1..=total {
            for failure in [Failure::Error, Failure::Crash, Failure::Lands] {
                let store = failing(&secret, step, failure);
                let deleted = delete(&store, WINDOWS, ACCOUNT);
                store.recover();
                let loaded = load(&store, WINDOWS, ACCOUNT).unwrap();
                // A delete succeeds exactly when the account's item is gone.
                match deleted {
                    Ok(()) => assert_eq!(loaded, None),
                    Err(_) => assert_eq!(loaded, Some(secret.clone())),
                }
                delete(&store, WINDOWS, ACCOUNT).unwrap();
                assert!(store.names().is_empty());
            }
        }
    }
}

#[test]
fn a_crash_after_the_commit_leaves_the_new_secret_and_chunks_for_a_sign_out() {
    // The previous item, five chunks and the commit, then the clean-up.
    let store = failing(&long('p'), 8, Failure::Crash);
    let previous: BTreeSet<String> = store.names();
    assert_eq!(save(&store, WINDOWS, ACCOUNT, &long('n')), Ok(()));
    store.recover();
    assert_eq!(load(&store, WINDOWS, ACCOUNT), Ok(Some(long('n'))));
    let names = store.names();
    assert!(previous.iter().all(|name| names.contains(name)));
    // A later save removes only the generation it replaces.
    save(&store, WINDOWS, ACCOUNT, SHORT).unwrap();
    assert_eq!(store.names().len(), 1 + 5);
    delete(&store, WINDOWS, ACCOUNT).unwrap();
    assert!(store.names().is_empty());
}

#[test]
fn of_two_interleaved_saves_the_last_committed_stays_readable() {
    let first = long('a');
    let total = steps(&long('p'), |store| {
        save(store, WINDOWS, ACCOUNT, &first).unwrap()
    });
    // Before the commit at step 7 the other save commits first.
    for (step, other) in (1..=total).flat_map(|step| [(step, long('b')), (step, SHORT.to_owned())])
    {
        let store = Memory::windows();
        save(&store, WINDOWS, ACCOUNT, &long('p')).unwrap();
        let interloper = other.clone();
        store.before_step(step, move |store| {
            save(store, WINDOWS, ACCOUNT, &interloper).unwrap();
        });
        save(&store, WINDOWS, ACCOUNT, &first).unwrap();
        let last = if step <= 7 { &first } else { &other };
        assert_eq!(
            load(&store, WINDOWS, ACCOUNT).as_ref(),
            Ok(&Some(last.clone()))
        );
        delete(&store, WINDOWS, ACCOUNT).unwrap();
        assert!(store.names().is_empty());
    }
}

#[test]
fn a_save_racing_a_delete_ends_signed_out_failed_closed_or_with_the_save() {
    let first = long('a');
    let total = steps(&long('p'), |store| {
        save(store, WINDOWS, ACCOUNT, &first).unwrap()
    });
    let mut outcomes = BTreeSet::new();
    for step in 1..=total {
        let store = Memory::windows();
        save(&store, WINDOWS, ACCOUNT, &long('p')).unwrap();
        store.before_step(step, |store| {
            delete(store, WINDOWS, ACCOUNT).unwrap();
        });
        let _ = save(&store, WINDOWS, ACCOUNT, &first);
        let loaded = load(&store, WINDOWS, ACCOUNT);
        assert!([Ok(Some(first.clone())), Ok(None), Err(KeychainError)].contains(&loaded));
        outcomes.insert(format!("{:?}", loaded.map(|secret| secret.is_some())));
        delete(&store, WINDOWS, ACCOUNT).unwrap();
        assert!(store.names().is_empty());
    }
    let expected = ["Err(KeychainError)", "Ok(false)", "Ok(true)"];
    assert_eq!(outcomes, expected.map(str::to_owned).into());
}
