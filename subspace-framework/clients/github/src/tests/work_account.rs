use crate::{pick_work_login, verified_work_login};

#[test]
fn chosen_account_wins_over_an_emu_login_using_gh_spelling() {
    let logins = ["octo-dev_acme".into(), "octo-dev".into()];
    let client = super::client("http://127.0.0.1:0".into(), &super::token("fixture"))
        .with_work_account(Some("OCTO-DEV".into()));
    assert_eq!(
        pick_work_login(&logins, client.work_account.as_ref()),
        Some(&logins[1])
    );
}

#[test]
fn unknown_or_absent_choice_falls_back_to_the_first_emu_login() {
    let logins = ["octo-dev".into(), "first_acme".into(), "second_acme".into()];
    assert_eq!(pick_work_login(&logins, None), Some(&logins[1]));
    assert_eq!(
        pick_work_login(&logins, Some(&"unknown".into())),
        Some(&logins[1])
    );
}

#[test]
fn neither_a_known_choice_nor_an_emu_login_returns_none() {
    let logins = ["octo-dev".into(), "trailing_".into(), "invalid_--".into()];
    assert_eq!(pick_work_login(&logins, None), None);
    assert_eq!(pick_work_login(&logins, Some(&"unknown".into())), None);
    assert_eq!(pick_work_login(&[], None), None);
}

#[test]
fn selected_account_still_requires_the_same_credential_viewer() {
    assert_eq!(
        verified_work_login("octo-dev", "octo-dev").unwrap(),
        "octo-dev"
    );
    let error = verified_work_login("octo-dev", "other-account").unwrap_err();
    assert_eq!(
        error.to_string(),
        "The selected work GitHub credential belongs to another account."
    );
}
