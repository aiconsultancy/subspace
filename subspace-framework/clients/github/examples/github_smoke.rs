//! Read-only smoke run against github.com: lists the account logins, counts
//! each section for the active account and loads one pull request's diff
//! metadata. Prints no tokens and makes no writes.
//!
//! `cargo run -q -p subspace-client-github --example github_smoke`

use std::sync::Arc;

use subspace_client_github::Client;
use subspace_client_github::model::{ItemKind, short_login};
use subspace_core::config;
use subspace_core::credentials::Credentials;

fn main() {
    let config = config::path()
        .map(|path| config::load(&path).0)
        .unwrap_or_default();
    let client = Client::new(Arc::new(Credentials::from_system(&config)));
    let login = active_login(&client);
    let first_pr = print_sections(&client, &login);
    print_diff(&client, &login, first_pr.as_deref());
}

/// The active account, or the first one. Exits when gh knows none.
fn active_login(client: &Client) -> String {
    let accounts = match client.accounts() {
        Ok(accounts) => accounts,
        Err(error) => {
            eprintln!("accounts: {error}");
            std::process::exit(1);
        }
    };
    println!("accounts: {}", accounts.logins.join(", "));
    let Some(login) = accounts
        .active
        .clone()
        .or_else(|| accounts.logins.first().cloned())
    else {
        eprintln!("no account");
        std::process::exit(1);
    };
    println!("active: {login} (short: {})", short_login(&login));
    login
}

/// Counts every section and returns the first pull request it saw.
fn print_sections(client: &Client, login: &str) -> Option<String> {
    let mut first_pr = None;
    for (id, result) in client.sections(login) {
        match result {
            Ok(section) => {
                println!(
                    "section {id}: {} items of {}",
                    section.items.len(),
                    section.total
                );
                if first_pr.is_none() {
                    first_pr = section
                        .items
                        .iter()
                        .find(|item| item.kind == ItemKind::Pr)
                        .map(|item| item.key.clone());
                }
            }
            Err(error) => println!("section {id}: error: {error}"),
        }
    }
    first_pr
}

fn print_diff(client: &Client, login: &str, key: Option<&str>) {
    let Some(key) = key else {
        println!("diff: no pull request in any section");
        return;
    };
    match client.diff(login, key) {
        Ok(diff) => println!(
            "diff: a pull request with {} files of {} (merge base {}…, head {}…)",
            diff.files.len(),
            diff.total,
            &diff.base_sha[..7],
            &diff.head_sha[..7]
        ),
        Err(error) => println!("diff: error: {error}"),
    }
}
