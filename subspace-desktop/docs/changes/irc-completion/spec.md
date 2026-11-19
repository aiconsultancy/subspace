# Spec: IRC completion

## Shared list (`subspace_dashboard::chat_input::completion`)

- `CompletionList` shows `CompletionItem`s with Kit's `Command` list, with
  search and filtering off, and keeps the highlighted position. A new set of
  items keeps the highlight where it was if it still fits. An empty set hides
  the list.
- Items: `CompletionItem::new(label)` is a plain row, as in the agent
  chat's `/command` rows. `CompletionItem::command(label, hint, detail)`
  adds the arguments, muted, and a one-line description at the end of the
  row, muted and truncated. `CompletionItem::person(name, mark, away)` adds
  the shared member initials (`avatar::member`) and a muted mark at the end
  of the row, such as a mode prefix `@` or `+`, and fades the row while
  away. The mark sits at the end, where the member list shows it, and not
  before the nick, where it would read as the `@` of a mention. `spoken`
  sets the accessible name, which Kit reads from the row's label.
- Keys come from the chat input. The owner calls
  `ChatInput::set_completing(true)` while the list is open, which adds the
  `SubspaceChatSlash` context. Up and Down emit `Previous` and `Next`, Tab
  and Enter emit `Complete`, and Escape emits `Cancel`. Enter is now bound
  in that context, ahead of the text area's own Enter, so choosing never
  also types a line break. A click on a row emits `CompletionEvent::Accept`.
  The list never takes focus, and the owner returns focus to the input after
  a choice.
- `ChatInput::cursor` and `ChatInput::replace_range` complete the word
  before the cursor as one undoable edit and leave the cursor after it.
  `word_before(text, cursor)` gives that word's range.

## IRC triggers (`spaces/irc/src/compose/suggest.rs`)

The word is the text from the last whitespace before the cursor to the
cursor, so completion works in the middle of a message.

| Typed | Offers | Inserts |
| --- | --- | --- |
| `@` + prefix | The buffer's members, or the other person of a private conversation | `@nick ` |
| `/` + prefix as the first word (not `//`) | `COMMANDS` whose name or alias starts with it, in catalogue order, each with arguments and a description | `/name ` |
| The first argument of `/join` | The network's channels | `#channel ` |
| The first argument of `/part`, `/topic` | The channels you are in | `#channel ` |
| The first argument of `/msg`, `/query`, `/notice`, `/whois`, `/invite`, `/kick` | Everyone on the network (channel members and private conversations) | `nick ` |
| `#` + prefix elsewhere | The channels you are in | `#channel ` |

- Matching is a prefix match that ignores case with RFC 1459 folding: letters
  ignore case, and `[]\~` equal `{}|^`. The board is not told the server's
  `CASEMAPPING`, and RFC 1459 folding matches at least as much as `ascii`.
  A channel also matches without its `#`, so `/join ship` offers
  `#ship-galactica`.
- People are ordered by who spoke most recently in the buffer (the last 500
  lines, messages only), then by folded nick. Your own nick is never offered.
  A list shows at most 50 people.
- People rows show initials, the nick and the highest mode prefix (muted, at
  the end).
  Away members are dimmed. Their accessible name adds "operator", "voiced"
  or "away".
- After a choice, the edit is checked again. `/join ` therefore opens the
  channel list straight away, and `@galactica-captain ` closes the list.

## Commands (`subspace_client_irc::COMMANDS`)

`join` (`j`), `part` (`leave`), `msg`, `query`, `me`, `notice`, `topic`,
`nick`, `away`, `whois`, `invite`, `kick`, `mode`, `quit`, `raw` (`quote`).
Each has its arguments, a description and what its first argument names
(`FirstArgument`). The parser's usage errors now come from the same table.

## Classic Tab completion (`spaces/irc/src/compose/complete.rs`)

With no list open, Tab completes the word before the cursor to the first
person, in the same order, whose nick starts with it. At the start of the line
it adds `: `, elsewhere a space. The edit replaces the word in place, so the
rest of the draft and the cursor stay.

## Capture steps

The debug-only capture feature adds `type:<text>`, which sends each
character as a key press, and `key:<keystrokes>`, which sends
space-separated keystrokes such as `down tab`. Both go to the first window.
