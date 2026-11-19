//! Badge initials.

use subspace_framework::client_github::model::{GhUser, short_login};

/// "Example, Alex (Team)" and "Alex Example" both give "AE".
fn initials_of(display_name: &str) -> String {
    let clean = without_brackets(display_name);
    let (first, last) = first_and_last(clean.trim());
    letters_of(first, last)
}

/// The name without any bracketed part, which is noise in a display name.
fn without_brackets(display_name: &str) -> String {
    let mut clean = String::new();
    let mut depth = 0usize;
    for c in display_name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                clean.push(' ');
            }
            _ if depth == 0 => clean.push(c),
            _ => {}
        }
    }
    clean
}

/// The first and last name, in either "Last, First" or "First Last" order.
fn first_and_last(clean: &str) -> (&str, &str) {
    match clean.split_once(',') {
        Some((last, first)) if !last.trim().is_empty() => (
            first.split_whitespace().next().unwrap_or(""),
            last.split_whitespace().next().unwrap_or(""),
        ),
        _ => {
            let words: Vec<&str> = clean.split_whitespace().collect();
            let first = words.first().copied().unwrap_or("");
            let last = if words.len() > 1 {
                words.last().copied().unwrap_or("")
            } else {
                ""
            };
            (first, last)
        }
    }
}

/// The leading letter of each name in upper case, or "?" when there is none.
fn letters_of(first: &str, last: &str) -> String {
    let letters: String = first
        .chars()
        .take(1)
        .chain(last.chars().take(1))
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() {
        "?".into()
    } else {
        letters
    }
}

/// The badge initials of a user: from the name, else the short login split on
/// dashes, underscores, dots and case changes (`badge`).
pub fn user_initials(user: &GhUser) -> String {
    match user
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => initials_of(name),
        None => initials_of(&words_of_login(&user.login)),
    }
}

/// A login as words: the part after any slash, split on separators and on
/// changes from lower case to upper case.
fn words_of_login(login: &str) -> String {
    let login = short_login(login);
    let login = login.rsplit('/').next().unwrap_or(login);
    let mut source = String::new();
    let mut previous_lower = false;
    for c in login.chars() {
        if previous_lower && c.is_ascii_uppercase() {
            source.push(' ');
        }
        previous_lower = c.is_ascii_lowercase();
        source.push(if matches!(c, '-' | '_' | '.') { ' ' } else { c });
    }
    source
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::model::GhUser;

    use super::{initials_of, user_initials};

    #[test]
    fn initials_from_names_and_logins() {
        assert_eq!(initials_of("Example, Alex (Team)"), "AE");
        assert_eq!(initials_of("Alex Example"), "AE");
        assert_eq!(initials_of(""), "?");
        let login = GhUser {
            login: "Alex-J-Example_emu".into(),
            name: None,
        };
        assert_eq!(user_initials(&login), "AE");
        let camel = GhUser {
            login: "annPerkins".into(),
            name: None,
        };
        assert_eq!(user_initials(&camel), "AP");
    }
}
