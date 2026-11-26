# Command-line interface

Commands run before the desktop starts. No window or terminal is required for
reads. The CLI and MCP use the same active-board selection, configuration,
search environment and capability catalog. Tools that act on the running
app's live state have no commands: `subspace irc` does not exist, because the
IRC tools need the board's connections (IRC is the Subspace App's Channels
Space).

```sh
subspace github --help
subspace github search --repo owner/repo --query is:pr --json
subspace github item --key owner/repo#7 --json
subspace board_help get --board_id github --json
subspace web_read --url https://docs.example.com/page --yes --json
```

Action commands drop a redundant board prefix and use kebab-case. For example,
`github_item` becomes `github item`, and `github_update_pr` becomes
`github update-pr`. Original action names
remain accepted as hidden aliases, so `subspace github github_item` still works.
Aliases do not appear in help or completion. A name collision rejects the
generated command tree rather than selecting one action silently, and so
does a command name or alias that would start with `-`, such as an action
`notes___help`, which would otherwise read as `--help`.

Each action field becomes a flag with the same name. Strings are plain arguments; numbers,
booleans, arrays and objects use JSON values. Quote JSON to protect it from the
shell. Help and completion come from the same declarations.

```sh
subspace github update-pr --key owner/repo#7 \
  --fields '{"title":"Fix the cache key"}' --yes --json
```

Writes require an interactive confirmation or explicit `--yes`. `--json` never
approves a write. Validation and preflight still run with `--yes`; one approval
covers one batch. Credential-like write previews are refused by the shared tool.
Without a terminal and without `--yes`, the command refuses before calling the
service. A terminal prompt shows the shared batch summary.

To submit several operations to the same write tool, use `--operations-file`:

```json
[
  {"action":"github_comment","key":"owner/repo#7","body":"Reviewed."},
  {"action":"github_comment","key":"owner/repo#8","body":"Reviewed."}
]
```

```sh
subspace github comment --operations-file comments.json --yes --json
```

The file must contain 1 to 20 operations and be at most 1 MiB. Do not combine
the file with operation-field flags. The tool validates the complete batch and
preflights all targets before the first write. Failed writes are not retried.
JSON operation objects retain their original action names, as MCP does.

`--json` prints the tool result without a CLI wrapper: the same JSON value as
MCP `structuredContent`. Default output is indented JSON text. Diagnostics and
confirmation prompts go to standard error. Exit codes are **0** for success,
**1** for a handler failure or refused confirmation, and **2** for invalid
arguments or routes. Inspect batch results after a partial service failure.

## Completion

```sh
eval "$(subspace completion bash)"
subspace completion zsh
subspace completion fish
```

Shared capabilities such as `board_help` and `web_read` are root commands. Only
enabled, compiled-in board capabilities are generated. Terminal has no
capabilities and retains the `NoCliForBoard` rejection. Additional Copilot Agents reads
appear automatically when their capabilities land.

## Offline verification

`make interfaces` runs the real product binary with a synthetic home and state,
configuration fixtures and closed standard input. Helpers `az`, `gh`, `kubectl`,
`sudo` and `copilot` are forbidden. The board-command tests compare a fixture
result with MCP over an in-memory protocol connection and check approval,
preflight and bounded batch files.

To refresh text evidence, set `SUBSPACE_CLI_EVIDENCE_DIR` to an absolute output
directory before `make interfaces`.

## Product command composition

`ProductDescriptor::command_mount` selects where generated Space commands
mount. Use `CommandMount::Prefixed`, the default, for `<binary> <space>
<command>`. `CommandMount::Root` requires exactly one declared Space and
mounts its actions as `<binary> <command>`. Validation rejects Root with zero
or several Spaces. Shared capabilities keep their root names in both modes:
Root mode flattens only the tools the declared Space provides, so a shared
`web_read` stays `web_read` even in a product whose one Space is `web`.
Original tool action names remain hidden aliases. For example, a `notes_read`
tool with a `notes_list` action becomes `example-app list` in Root mode, with
`example-app notes_list` still accepted.

`ProductBuild::pre_startup` is an optional
`fn(&[String]) -> Option<ExitCode>`. The shell calls it after startup `PATH`
setup but before theme validation, configuration reads, update cleanup or
GPUI initialisation. Arguments exclude the binary name. `Some(code)` returns
that exit code without continuing startup; `None` continues the normal path.
The hook runs whenever supplied, for every startup route, with no capability
gate. `ProductBuild::new(descriptor, version, build_info)` leaves it unset and
disables the optional host integrations.

For a product-owned CLI, pass a callback to `shell::launch` that constructs
`board_command::Composition` and calls its `run` method:

```rust,no_run
use std::process::ExitCode;
use subspace_framework::{agent_tools::ToolEnvironment, board_command, mcp};
use subspace_framework::board_command::usage::Spec;
use subspace_framework::product::ProductContext;

fn product_tree(context: &ProductContext) -> Result<Spec, String> {
    let mut spec = board_command::root_tree(context)?;
    let extra = "cmd \"list\" help=\"List local notes\""
        .parse::<Spec>().map_err(|error| error.to_string())?;
    spec.cmd.subcommands.extend(extra.cmd.subcommands);
    Ok(spec)
}

fn fallback(_: &ProductContext, args: &[String]) -> ExitCode {
    if args == ["list"] {
        println!("No local notes");
        ExitCode::SUCCESS
    } else {
        eprintln!("Unknown command: {}", args.join(" "));
        ExitCode::from(2)
    }
}

fn commands(
    context: &ProductContext,
    boards: &[mcp::Board],
    environment: &ToolEnvironment,
    args: &[String],
) -> ExitCode {
    board_command::Composition::new(
        product_tree,
        board_command::complete_word,
        fallback,
    )
    .with_specialized_tools(&["notes_read"])
    .run(context, boards, environment, args)
}
```

The product supplies these functions:

- Tree: `fn(&ProductContext) -> Result<board_command::usage::Spec, String>`.
- Completion: `fn(&board_command::usage::Spec, &[String]) -> Vec<String>`.
- Fallback: `fn(&ProductContext, &[String]) -> ExitCode`.

The tree owns the root declarations. `board_command::root_tree(context)`
provides the Framework's completion, MCP and optional upgrade declarations
as a starting point. When the tree keeps that `completion` declaration,
Composition prints the Framework's shell scripts for it, as the standard CLI
does; a product that declares a different `completion` handles it in its
fallback. The fallback handles the product's other declared commands;
startup still handles MCP and upgrade. The Framework adds generated
commands without replacing product commands or their aliases. Nor may a
generated command take a word start-up answers before any command runs.
Start-up claims `about`, `--about`, `--version` and `__product_metadata`
only as the sole argument, so a generated leaf command or alias may not use
them, but a Space group may: `example-app about read` runs the `about`
Space's `read` action while `example-app about` still prints the build
information. It claims `mcp`, and `upgrade` when the product has an update
source, with any arguments, so no generated command, group or Space id may
use them. The same holds for `__complete-word`, which the command runner
answers before it parses, whether or not the product's tree declares it. A
collision fails with exit code 1.
Flags the tree declares at the root, such as a global `--profile <NAME>`,
parse on a generated command's line but never reach its tool, which gets
only its own fields. usage-lib keys parsed flags by name, so a generated
command may not declare a flag that shares a name or any spelling with one
of those flags: long, short, hidden alias or `negate` token, clause flags
included. `flag "body: --text <TEXT>"` collides with a generated `--body`,
and `flag "--pretty" negate="--json"` with a generated `--json`; a collision
fails with exit code 1. Every generated command declares `--json` and
`--yes`, a batched one `--operations-file`, and each its tool's fields, so
a product cannot use those names at the root; declare such a flag on the
product's own commands instead. The Framework's own root tree declares no
root flags.
`with_specialized_tools` excludes exact tool identities from CLI generation
only; MCP still serves them.

The hidden `__complete-word -- <words>` route gives completion the merged
tree and the words after `--`. `board_command::complete_word` is the standard
completion callback, including command names, flags and declared choices.
Fallback receives the original arguments for valid product-owned commands
and unknown root commands. Invalid arguments on declared commands return
exit code 2 and do not reach fallback. A root command is unknown when the
parser stopped on a non-flag word nothing took and the words before it,
parsed on their own with every check the parser makes, are a valid line of
root flags: no command, no root argument, no `--` separator and no error, a
missing `required` root flag included. So `example-app --verbose external` reaches fallback
when the tree declares a root `--verbose`, while an unknown flag, an invalid
flag value (`--dry-run=bogus` on a boolean flag, a value outside its
choices) or a word a root argument took or refused is reported. A tree that
declares a `default_subcommand` hands every such word to that command, so
its refusals are that command's errors and none reaches fallback. A `mount`
at the root likewise owns the words it does not declare, and none reaches
fallback. The stop's position comes from a re-read that runs no mount
discovery and no `choices run=` scripts; validating the words before it
runs the scripts of the root flags among them a second time. If the served
catalogue cannot be built, for example because the only Space is turned
off, Composition prints `Space commands unavailable: <reason>` to standard
error and runs the product's own tree with no generated commands, under the
same rules, so product-owned commands still run and invalid arguments still
return exit code 2. Generated-tree errors do not fall back. The standard
`board_command::run` is the same runner with the Framework's tree and no
product commands: it reports a catalogue failure with exit code 1 and an
unknown root command as `Unknown command: <words>` with exit code 2.
