//! Joined channels and their members, keyed by case-folded names.

use std::collections::BTreeMap;

use crate::model::Member;

#[derive(Debug, Clone, Default)]
pub struct Channel {
    /// The name as the server spelled it on JOIN.
    pub name: String,
    pub topic: String,
    /// Members by folded nick.
    pub members: BTreeMap<String, Member>,
    /// A `NAMES` reply in progress.
    pub names: Option<BTreeMap<String, Member>>,
    /// A `WHO` reply is in progress.
    pub who: bool,
}

/// A member from a `NAMES` entry such as `@+nick!user@host`.
pub fn names_entry(entry: &str, symbols: &[char]) -> Member {
    let nick_start = entry.find(|c| !symbols.contains(&c)).unwrap_or(entry.len());
    let (prefixes, rest) = entry.split_at(nick_start);
    let nick = rest.split('!').next().unwrap_or(rest);
    Member {
        nick: nick.to_string(),
        prefixes: prefixes.to_string(),
        away: false,
        account: None,
    }
}

/// `prefixes` with `symbol` added or removed, ordered as `symbols`.
pub fn change_prefix(prefixes: &str, symbol: char, adding: bool, symbols: &[char]) -> String {
    symbols
        .iter()
        .filter(|s| match **s == symbol {
            true => adding,
            false => prefixes.contains(**s),
        })
        .collect()
}

/// Members sorted by their highest prefix, then nick (case-insensitive).
pub fn sorted(members: &BTreeMap<String, Member>, symbols: &[char]) -> Vec<Member> {
    let rank = |member: &Member| {
        member
            .prefixes
            .chars()
            .next()
            .and_then(|first| symbols.iter().position(|s| *s == first))
            .unwrap_or(symbols.len())
    };
    let mut list: Vec<(usize, &String, &Member)> = members
        .iter()
        .map(|(key, member)| (rank(member), key, member))
        .collect();
    list.sort();
    list.into_iter()
        .map(|(_, _, member)| member.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{change_prefix, names_entry, sorted};

    const SYMBOLS: [char; 3] = ['@', '%', '+'];

    #[test]
    fn parses_multi_prefix_and_userhost_entries() {
        let member = names_entry("@+dan!d@host", &SYMBOLS);
        assert_eq!(member.nick, "dan");
        assert_eq!(member.prefixes, "@+");
        assert_eq!(names_entry("plain", &SYMBOLS).prefixes, "");
    }

    #[test]
    fn changes_prefixes_in_rank_order() {
        assert_eq!(change_prefix("+", '@', true, &SYMBOLS), "@+");
        assert_eq!(change_prefix("@+", '@', false, &SYMBOLS), "+");
        assert_eq!(change_prefix("", '+', false, &SYMBOLS), "");
    }

    #[test]
    fn sorts_by_rank_then_nick() {
        let members: BTreeMap<_, _> = ["+bob", "carol", "@zed", "alice"]
            .into_iter()
            .map(|entry| {
                let member = names_entry(entry, &SYMBOLS);
                (member.nick.to_lowercase(), member)
            })
            .collect();
        let nicks: Vec<_> = sorted(&members, &SYMBOLS)
            .into_iter()
            .map(|member| member.nick)
            .collect();
        assert_eq!(nicks, ["zed", "bob", "alice", "carol"]);
    }
}
