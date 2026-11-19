//! The product's root commands: shell completion, plus `mcp` and `upgrade`,
//! which startup routes first and are declared here for help and completion.

use subspace_product::{MCP_USAGE, ProductContext, UPGRADE_USAGE};

const COMPLETION_USAGE: &str = r#"cmd "completion" effect="read" help="Print a shell completion script for the current shell" {
  arg "<shell>" help="Target shell" {
    choices "bash" "zsh" "fish"
  }
}
cmd "__complete-word" hide=#true effect="read" help="Internal: list completion candidates for a partial command line" {
  arg "[words]..." help="Words after the shell completion separator"
}"#;

pub(crate) const COMPLETION: &str = "completion";
pub(crate) const COMPLETE_WORD: &str = "__complete-word";

/// The root tree for `context`'s binary, before generated board routes.
pub(crate) fn tree(context: &ProductContext) -> Result<usage::Spec, String> {
    let upgrade = if context.product.update.is_some() {
        UPGRADE_USAGE
    } else {
        ""
    };
    format!(
        "bin {:?}\nversion {:?}\n{COMPLETION_USAGE}\n{MCP_USAGE}\n{upgrade}\n",
        context.product.binary_name.as_str(),
        context.version,
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

/// Candidate next words for a partial command line. `words` excludes the
/// program name; the last element is the (possibly empty) word under the cursor.
pub(crate) fn complete_word(spec: &usage::Spec, words: &[String]) -> Vec<String> {
    let Some((prefix, preceding)) = words.split_last() else {
        return Vec::new();
    };
    // A flag before the cursor still waits for its value, which makes the
    // partial line unparseable; offer that flag's declared choices instead.
    if !prefix.starts_with('-')
        && let Some((flag_word, before_flag)) = preceding.split_last()
        && flag_word.starts_with('-')
    {
        return flag_choices(spec, before_flag, flag_word, prefix);
    }
    let argv: Vec<String> = std::iter::once(spec.bin.clone())
        .chain(preceding.iter().cloned())
        .collect();
    let Ok(output) = usage::parse::Parser::new(spec).explain(&argv) else {
        return Vec::new();
    };
    let mut candidates: Vec<String> = if prefix.starts_with('-') {
        output
            .completion_flags()
            .into_keys()
            .filter(|key| key.starts_with(prefix.as_str()))
            .collect()
    } else {
        let choices = output
            .next_arg
            .as_ref()
            .and_then(|arg| arg.choices.as_ref())
            .map(|choices| choices.choices.clone())
            .unwrap_or_default();
        output
            .cmd
            .subcommands
            .iter()
            .filter(|(_, command)| !command.hide)
            .map(|(name, _)| name.clone())
            .chain(choices)
            .filter(|word| word.starts_with(prefix.as_str()))
            .collect()
    };
    candidates.sort();
    candidates.dedup();
    candidates
}

fn flag_choices(
    spec: &usage::Spec,
    before_flag: &[String],
    flag_word: &str,
    prefix: &str,
) -> Vec<String> {
    let argv: Vec<String> = std::iter::once(spec.bin.clone())
        .chain(before_flag.iter().cloned())
        .collect();
    let Ok(output) = usage::parse::Parser::new(spec).explain(&argv) else {
        return Vec::new();
    };
    let mut candidates: Vec<String> = output
        .completion_flags()
        .get(flag_word)
        .and_then(|flag| flag.arg.as_ref())
        .and_then(|arg| arg.choices.as_ref())
        .map(|choices| {
            choices
                .choices
                .iter()
                .filter(|choice| choice.starts_with(prefix))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    candidates.sort();
    candidates.dedup();
    candidates
}

/// A shell init script that calls this binary's hidden `__complete-word` route.
pub(crate) fn init_script(bin: &str, shell: &str) -> Option<String> {
    let bin_fn = bin.replace(['-', ' '], "_");
    match shell {
        "bash" => Some(format!(
            r#"_{bin_fn}_complete() {{
  local cur="${{COMP_WORDS[COMP_CWORD]}}"
  local words=("${{COMP_WORDS[@]:1:COMP_CWORD}}")
  COMPREPLY=($(compgen -W "$(command {bin} __complete-word -- "${{words[@]}}" 2>/dev/null)" -- "$cur"))
}}
complete -F _{bin_fn}_complete {bin}
"#
        )),
        "zsh" => Some(format!(
            r#"#compdef {bin}
_{bin_fn}_complete() {{
  local -a words=("${{(@)words[2,CURRENT]}}")
  local -a candidates
  candidates=("${{(@f)$(command {bin} __complete-word -- "${{words[@]}}" 2>/dev/null)}}")
  compadd -a candidates
}}
compdef _{bin_fn}_complete {bin}
"#
        )),
        "fish" => Some(format!(
            r#"function __{bin_fn}_complete_word
    set -l tokens (commandline -opc)
    set -l cur (commandline -ct)
    command {bin} __complete-word -- $tokens[2..-1] $cur 2>/dev/null
end
complete -c {bin} -f -a '(__{bin_fn}_complete_word)'
"#
        )),
        _ => None,
    }
}
