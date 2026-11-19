use regex::Regex;

#[derive(Clone)]
pub(super) struct Home(Regex);

impl Home {
    pub(super) fn new(home: String) -> Option<Self> {
        let home = home.trim_end_matches(['/', '\\']);
        if home.is_empty() {
            return None;
        }
        let pattern = regex::escape(home);
        let pattern = if home.contains('\\') || home.as_bytes().get(1) == Some(&b':') {
            let parts: Vec<_> = home.split(['/', '\\']).map(regex::escape).collect();
            format!("(?i:{})", parts.join(r"[\\/]"))
        } else {
            pattern
        };
        Regex::new(&pattern).ok().map(Self)
    }

    pub(super) fn matches(&self, value: &str) -> bool {
        let value = value.trim_end_matches(['/', '\\']);
        self.0
            .find(value)
            .is_some_and(|found| found.start() == 0 && found.end() == value.len())
    }

    pub(super) fn replace(&self, text: &str) -> String {
        let mut result = String::new();
        let mut cursor = 0;
        for found in self.0.find_iter(text) {
            if !has_boundaries(text, found.start(), found.end()) {
                continue;
            }
            result.push_str(&text[cursor..found.start()]);
            result.push('~');
            cursor = found.end();
        }
        result.push_str(&text[cursor..]);
        result
    }
}

fn has_boundaries(text: &str, start: usize, end: usize) -> bool {
    let left = text[..start].chars().next_back();
    let right = text[end..].chars().next();
    let left_boundary =
        left.is_none_or(|value| !value.is_alphanumeric() && !"_/\\.-".contains(value));
    let right_boundary = right
        .is_none_or(|value| !value.is_alphanumeric() && !"_.-".contains(value))
        || text[end..]
            .strip_prefix('.')
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
    left_boundary && right_boundary
}
