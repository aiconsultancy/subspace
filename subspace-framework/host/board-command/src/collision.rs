//! Name checks on generated commands: every spelling is unique in its
//! namespace, and none takes a word the product tree or start-up owns.

use std::collections::{BTreeSet, HashSet};

use crate::tree::{Route, single_alias};

/// Start-up takes these words before the command runner sees them, and the
/// runner takes the completion word before it parses, so a generated command
/// or alias with one of them would never run. A leaf may
/// be invoked alone, so it may not use any word start-up claims; a group
/// always has a word after it, so only the words start-up claims with
/// arguments are closed to it.
pub(crate) fn reject_host_words(generated: &usage::Spec, updates: bool) -> Result<(), String> {
    let leaf = subspace_product::reserved_leaf_words(updates);
    let group = subspace_product::reserved_group_words(updates);
    let taken = generated
        .cmd
        .subcommands
        .iter()
        .find_map(|(name, command)| {
            let reserved = if command.subcommands.is_empty() {
                &leaf
            } else {
                &group
            };
            std::iter::once(name)
                .chain(&command.aliases)
                .chain(&command.hidden_aliases)
                .find(|word| reserved.contains(&word.as_str()))
        });
    match taken {
        Some(name) => Err(format!(
            "Generated command collides with a reserved root word: {name}"
        )),
        None => Ok(()),
    }
}

/// A product's root flags parse on a generated command's line, and
/// usage-lib keys the parsed flags by name (`SpecFlag` compares and hashes
/// by name alone), so a product flag that shares a name or a spelling with a
/// generated flag would shadow it or hand the tool its value. A generated
/// command may declare neither.
pub(crate) fn reject_flag_overlap(
    product: &usage::Spec,
    generated: &usage::Spec,
) -> Result<(), String> {
    let taken: BTreeSet<String> = command_flags(&product.cmd).flat_map(flag_keys).collect();
    let mut commands: Vec<&usage::SpecCommand> = generated.cmd.subcommands.values().collect();
    while let Some(command) = commands.pop() {
        commands.extend(command.subcommands.values());
        if let Some(key) = command_flags(command)
            .flat_map(flag_keys)
            .find(|key| taken.contains(key))
        {
            return Err(format!(
                "Generated command flag collides with a product flag: {key}"
            ));
        }
    }
    Ok(())
}

/// The flags a command declares, its clause's included, as usage-lib
/// gathers them.
fn command_flags(command: &usage::SpecCommand) -> impl Iterator<Item = &usage::SpecFlag> {
    command
        .flags
        .iter()
        .chain(command.clause.iter().flat_map(|clause| &clause.flags))
}

/// Every key usage-lib finds a flag by: each typed spelling (`--long`, `-s`,
/// hidden aliases and the `negate` token), then its name. Spellings start
/// with `-` and names do not, so the two never meet in one set.
fn flag_keys(flag: &usage::SpecFlag) -> Vec<String> {
    flag.long
        .iter()
        .chain(&flag.hidden_aliases)
        .map(|long| format!("--{long}"))
        .chain(
            flag.short
                .iter()
                .chain(&flag.hidden_short_aliases)
                .map(|short| format!("-{short}")),
        )
        .chain(flag.negate.clone())
        .chain(std::iter::once(flag.name.clone()))
        .collect()
}

pub(crate) fn merge_spec(spec: &mut usage::Spec, extra: usage::Spec) -> Result<(), String> {
    let existing = root_names(spec);
    let generated = root_names(&extra);
    if let Some(name) = existing.intersection(&generated).next() {
        return Err(format!(
            "Generated command collides with existing command: {name}"
        ));
    }
    spec.cmd.subcommands.extend(extra.cmd.subcommands);
    // The parser memoises subcommand lookup; a tree callback may already
    // have filled it without the generated commands.
    spec.restamp();
    Ok(())
}

fn root_names(spec: &usage::Spec) -> BTreeSet<&String> {
    spec.cmd
        .subcommands
        .iter()
        .flat_map(|(name, command)| {
            std::iter::once(name)
                .chain(&command.aliases)
                .chain(&command.hidden_aliases)
        })
        .collect()
}

pub(crate) fn validate_names(routes: &[Route]) -> Result<(), String> {
    let spellings: Vec<Vec<String>> = routes.iter().flat_map(spellings).collect();
    if let Some(word) = flag_like(&spellings) {
        return Err(format!("Generated command name reads as a flag: {word}"));
    }
    match first_collision(&spellings) {
        Some(name) => Err(format!("Generated command collision: {name}")),
        None => Ok(()),
    }
}

/// A command word that starts with `-` can be typed where a flag is read,
/// and usage-lib selects a declared subcommand before its own `--help` and
/// `--version`, so a generated name or alias may not look like a flag at any
/// depth. This also keeps generated words off start-up's `--about` and
/// `--version`.
fn flag_like(spellings: &[Vec<String>]) -> Option<&String> {
    spellings
        .iter()
        .flatten()
        .find(|word| word.starts_with('-'))
}

/// Every path a route answers to: its canonical path, then its hidden aliases.
fn spellings(route: &Route) -> Vec<Vec<String>> {
    let mut spellings = vec![route.path.clone()];
    if let Some(action) = &route.action
        && Some(action) != route.path.last()
    {
        let mut alias = route.path.clone();
        alias.pop();
        alias.push(action.clone());
        spellings.push(alias);
    }
    if let Some(alias) = single_alias(route) {
        spellings.push(vec![alias.to_owned()]);
    }
    spellings
}

/// Group roots, leaf names and leaf aliases are one root namespace: a root
/// word that names both a group and a command, or two commands, collides.
fn first_collision(spellings: &[Vec<String>]) -> Option<String> {
    let groups: HashSet<&str> = spellings
        .iter()
        .filter(|path| path.len() > 1)
        .map(|path| path[0].as_str())
        .collect();
    let mut seen = HashSet::new();
    spellings.iter().find_map(|path| {
        let name = path.join(" ");
        let collides = path.iter().any(String::is_empty)
            || (path.len() == 1 && groups.contains(name.as_str()))
            || !seen.insert(name.clone());
        collides.then_some(name)
    })
}
