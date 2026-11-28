//! A signed-in account the tests switch, and reads that answer as whoever
//! is signed in. The other account and its decision are fictional.

use std::sync::{Arc, Mutex};

use serde_json::json;
use subspace_api_client::Error;
use subspace_api_client::model::DecisionListItem;

use super::{answer, item_value, keeps, me, project, refused};
use crate::load::{Answer, Directory, Found, Lease, Leased, Plan, Reader};

/// The other account's name and its one decision.
pub(crate) const OTHER_NAME: &str = "second-operator";
pub(crate) const OTHER_DECISION: &str = "e1";
const OTHER_USER: &str = "user-second";

/// Who is signed in for [`Accounts`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Who {
    /// The operator, who sees the eight fixture decisions.
    #[default]
    Operator,
    /// Another account, which sees one decision of its own.
    Other,
    SignedOut,
}

/// A signed-in account that tests switch, as the app's account generation
/// changes on a switch of account or organisation and on a sign-out. The
/// operator starts signed in.
#[derive(Clone, Default)]
pub(crate) struct Accounts {
    /// The account generation and who it is.
    signed_in: Arc<Mutex<(u64, Who)>>,
}

impl Accounts {
    /// Signs `who` in, retiring every lease taken before.
    pub(crate) fn switch(&self, who: Who) {
        let mut signed_in = self.signed_in.lock().unwrap();
        *signed_in = (signed_in.0 + 1, who);
    }

    fn current(&self) -> (u64, Who) {
        *self.signed_in.lock().unwrap()
    }

    /// Who is signed in, and a lease on that same account generation, as a
    /// read's client and its lease are one.
    fn client(&self) -> (Lease, Who) {
        let (taken, who) = self.current();
        let accounts = self.clone();
        (Lease::new(move || accounts.current().0 != taken), who)
    }
}

/// Reads that answer as whoever `accounts` has signed in when they run,
/// each under a lease on that account.
pub(crate) fn reader_for(accounts: &Accounts) -> Reader {
    let (list, detail) = (accounts.clone(), accounts.clone());
    Reader {
        list: Arc::new(move |plan: &Plan| {
            let (lease, who) = list.client();
            let answer = match who {
                Who::Operator => answer(plan),
                Who::Other => other_answer(plan),
                Who::SignedOut => refused(plan, Error::SignInNeeded),
            };
            Leased::Read(lease, answer)
        }),
        detail: Arc::new(move |id, _| {
            let (lease, who) = detail.client();
            let found = match who {
                Who::Operator => super::detail(id).ok_or(Error::NotFound),
                Who::Other => Err(Error::NotFound),
                Who::SignedOut => Err(Error::SignInNeeded),
            };
            Leased::Read(lease, found)
        }),
    }
}

/// The other account's answer: its own directory, project and decision.
fn other_answer(plan: &Plan) -> Answer {
    let mut value = item_value(
        OTHER_DECISION,
        "OPS",
        "Rotate the relay keys weekly",
        "accepted",
        "2026-11-17",
    );
    value["project"]["name"] = json!("Operations");
    value["madeByUserId"] = json!(OTHER_USER);
    value["recordedByUserId"] = json!(OTHER_USER);
    let item: DecisionListItem = serde_json::from_value(value).unwrap();
    let directory = Directory {
        me: me(OTHER_USER, OTHER_NAME),
        human_makers: Vec::new(),
        projects: vec![project("OPS", "Operations")],
        people: [(OTHER_USER.to_owned(), OTHER_NAME.to_owned())]
            .into_iter()
            .collect(),
    };
    let items = [item]
        .into_iter()
        .filter(|item| keeps(item, &plan.query))
        .collect();
    Answer {
        directory: plan.directory.then_some(Ok(directory)),
        found: Ok(Found {
            items,
            ..Found::default()
        }),
    }
}
