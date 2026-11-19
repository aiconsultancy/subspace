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
generated command tree rather than selecting one action silently.

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
