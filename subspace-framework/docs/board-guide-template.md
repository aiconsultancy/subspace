# Board guide template

Use one `spaces/<id>/help.md` as the source for in-app help and the docs site.
Replace the title with the board's UI name. Keep the following H2 headings in
this order. Use H3 headings for procedures and reference groups within them.
Do not omit a section when a board has no corresponding feature. State the
limit instead. Preserve exact UI labels, product names, keys and identifiers.

Every feature claim needs evidence in the board's implementation, connection
settings descriptors, tools, keyboard bindings or an isolated app check. Read
the existing guide and related change records before rewriting. Record source
paths in HTML comments where useful. State which live behavior was not checked.

Screenshots must come from a directly launched isolated dev binary with empty
HOME and XDG directories. Use synthetic data only. Register bounded WebP files
through the board's help-image declarations and add descriptive alt text. Do
not reuse screenshots from an installed app. If there is no bounded fixture
path, explain why this guide has no screenshot. Writers must not run the app,
Cargo or screenshot tools. The program owner captures all images.

The guide is one item of the [board checklist](board-checklist.md); see
[Help and docs](board-integration.md#help-and-docs) for how the guide reaches
the app and agents.

```markdown
# <Board name> guide

## Overview

State the board's purpose, availability and how to open its help.

## Before you start

List prerequisites, sign-in, credential sources and any required permissions.
Put warnings before the affected action. Do not include real account data.

## Main view

Describe navigation, rows, grouping, status, selection and dock panels.
Place a synthetic screenshot here if an approved fixture path exists.

## Toolbar and filters

List the exact controls, defaults, filter scope, refresh and grouping behavior.

## Details

Describe detail panels, tabs, loading, links and copy behavior.

## Actions

Give procedures for user-visible actions, confirmations and results.
Include relevant agent tools and their approval boundary.

## Keyboard shortcuts

List verified board bindings and any applicable shared navigation keys.
Distinguish a shortcut from an ordinary text input.

## Settings

Document the board's Settings page, defaults, save/restart behavior and secrets.
Distinguish connection settings from immediate board controls.

## Troubleshooting

Connect observable symptoms with specific checks or recovery actions.
Do not promise that a successful local check proves service authorization.

## Limits

State platform, data, refresh and action limits. Mark live behavior that was
not checked. Explain any omitted screenshots.
```
