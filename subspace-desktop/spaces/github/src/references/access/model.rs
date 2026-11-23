//! The access invariant, checked on every interleaving of six reads of one
//! pull request: every start order of two list sections, two detail reads
//! and two reference reads (as six start ticks), every outcome each kind
//! can have (a section succeeds or fails; a detail or reference read
//! succeeds, is refused or fails), every landing order, and with nothing,
//! a refresh, an unrelated account's sign-out, or both, between the third
//! and fourth landings. After every landing the resolver is refused exactly
//! when the most recently started read that has landed with an access
//! decision is a refusal.

use subspace_framework::contracts::github::ResolutionState;

use super::super::{References, forget_answers, mark_stale};
use super::{Decision, decide, is_refusal, refused};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Success,
    Refusal,
    Failure,
}

const KEY: (&str, &str) = ("octo-dev", "octo-org/api#421");

/// Whether a resolver lookup answers refused, never from what is held:
/// `held::known` answers `Unavailable` exactly while [`refused`], and
/// drops a held refusal answer a later success overtook.
fn resolver_refuses(references: &References, key: &(String, String)) -> bool {
    let held_refusal = references.answers.get(key).is_some_and(is_refusal);
    refused(references, key) || (held_refusal && refused(references, key))
}

/// A read landing as the paths land it: the decision, and a refusal kept
/// as the answer when it is the latest.
fn land(references: &mut References, key: &(String, String), started: u64, outcome: Outcome) {
    let decision = match outcome {
        Outcome::Success => Decision::Success,
        Outcome::Refusal => Decision::Refusal,
        Outcome::Failure => return,
    };
    if decide(references, key, started, decision) && decision == Decision::Refusal {
        references
            .answers
            .insert(key.clone(), ResolutionState::Unavailable);
    }
}

/// Every permutation of `0..n`.
fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for rest in permutations(n - 1) {
        for at in 0..=rest.len() {
            let mut order = rest.clone();
            order.insert(at, n - 1);
            all.push(order);
        }
    }
    all
}

/// Every outcome of six reads by start tick: each tick's read may be any
/// kind, so each may succeed, be refused or fail; a section's outcomes are
/// among these.
fn outcomes() -> Vec<[Outcome; 6]> {
    let each = [Outcome::Success, Outcome::Refusal, Outcome::Failure];
    let mut all = Vec::new();
    for code in 0..729usize {
        let mut picked = [Outcome::Failure; 6];
        let mut rest = code;
        for slot in &mut picked {
            *slot = each[rest % 3];
            rest /= 3;
        }
        all.push(picked);
    }
    all
}

/// What the invariant expects after `landed` (start positions) with
/// `outcomes`: refused when the latest-started deciding read refused.
fn expected(landed: &[usize], outcomes: &[Outcome; 6]) -> bool {
    landed
        .iter()
        .filter(|at| outcomes[**at] != Outcome::Failure)
        .max()
        .is_some_and(|latest| outcomes[*latest] == Outcome::Refusal)
}

/// Between the third and fourth landings: nothing, a refresh, an
/// unrelated account's sign-out, or both.
fn interrupt(references: &mut References, between: u8) {
    if between & 1 == 1 {
        references.refresh_due = true;
        mark_stale(references);
    }
    if between & 2 == 2 {
        forget_answers(references);
    }
}

/// Lands six reads in `order` with `outcomes`, checking the invariant
/// after each landing; how many checks passed.
fn play(outcomes: &[Outcome; 6], order: &[usize], between: u8) -> usize {
    let key = (KEY.0.to_owned(), KEY.1.to_owned());
    let mut references = References {
        refresh_due: true,
        ..References::default()
    };
    let mut landed = Vec::new();
    for (step, at) in order.iter().enumerate() {
        if step == 3 {
            interrupt(&mut references, between);
        }
        land(&mut references, &key, *at as u64 + 1, outcomes[*at]);
        landed.push(*at);
        assert_eq!(
            resolver_refuses(&references, &key),
            expected(&landed, outcomes),
            "outcomes by start {:?}, landing order {order:?}, between {between}",
            outcomes.map(|outcome| outcome as u8),
        );
    }
    landed.len()
}

#[test]
fn the_latest_started_deciding_read_decides_access_on_every_interleaving() {
    let orders = permutations(6);
    let checked: usize = outcomes()
        .iter()
        .flat_map(|outcomes| orders.iter().map(move |order| (outcomes, order)))
        .flat_map(|(outcomes, order)| (0..4u8).map(move |between| play(outcomes, order, between)))
        .sum();
    assert_eq!(checked, 729 * 720 * 4 * 6);
}
