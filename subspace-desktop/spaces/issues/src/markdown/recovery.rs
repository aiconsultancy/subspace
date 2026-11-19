//! Recover malformed fence syntax before applying normal Subspace API display rules.

use std::borrow::Cow;

use markdown::{ParseOptions, mdast::Node};

pub(super) fn parse<'a>(source: &'a str, parser: &ParseOptions) -> Option<(Cow<'a, str>, Node)> {
    if let Ok(tree) = markdown::to_mdast(source, parser) {
        return Some((Cow::Borrowed(source), tree));
    }
    let escaped = escape_fences(source);
    markdown::to_mdast(&escaped, parser)
        .ok()
        .map(|tree| (Cow::Owned(escaped), tree))
}

pub(super) fn fallback(source: &str, parser: &ParseOptions) -> String {
    let escaped = super::literal(source);
    if markdown::to_mdast(&escaped, parser).is_ok() {
        return escaped;
    }
    source
        .chars()
        .map(|c| format!("&#{};", u32::from(c)))
        .collect()
}

fn escape_fences(source: &str) -> String {
    const MIN_CODE_FENCE: usize = 3;
    const MIN_MATH_FENCE: usize = 2;
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(character) = chars.next() {
        if !matches!(character, '`' | '~' | '$') {
            result.push(character);
            continue;
        }
        let mut count = 1;
        while chars.next_if_eq(&character).is_some() {
            count += 1;
        }
        let minimum = if character == '$' {
            MIN_MATH_FENCE
        } else {
            MIN_CODE_FENCE
        };
        for _ in 0..count {
            if count >= minimum {
                result.push('\\');
            }
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::escape_fences;

    #[test]
    fn retry_escapes_complete_fence_runs_and_no_other_punctuation() {
        assert_eq!(
            escape_fences("[x] $5 $$ $$$ ~ ~~ ~~~ ~~~~ ` `` ``` ````"),
            "[x] $5 \\$\\$ \\$\\$\\$ ~ ~~ \\~\\~\\~ \\~\\~\\~\\~ ` `` \\`\\`\\` \\`\\`\\`\\`"
        );
    }
}
