# Intent: IRC completion

The owner asked: "We should be able to '@' people in IRC, there should be an
auto completed list. The same goes for commands if possible".

Their screenshot showed the composer holding `@gal`, with the caret after it
and no list, annotated "autocomplete please!". Above it, the Slack-style
transcript showed `@galactica-captain report`, with `@galactica-captain` as
a mention pill.

## Decisions

1. **Insert `@nick `, not `nick: `.** IRC has no mention syntax, and irssi
   and Halloy address people with a bare `nick: `. Subspace's transcript,
   however, shows `@nick` for a channel member as a mention pill, and the
   owner types `@`. Choosing a person therefore inserts `@nick` and a space.
   The recipient is still highlighted. Subspace's client, irssi, WeeChat and
   Halloy all match the nick as a whole word, and `@` is not a nick
   character.
2. **Keep classic Tab completion.** With no list open, `Tab` completes a bare
   nick before the cursor and adds `: ` at the start of the line, as before.
3. **One list.** The agent chat's `/` list moved its rendering and highlight
   into `subspace_dashboard::chat_input::completion`. The IRC composer uses
   the same list instead of a second popup. The agent chat's commands,
   filtering, keys and run-on-choice behaviour are unchanged.
4. **Only commands that work.** The command list lives in the IRC client
   beside the parser (`COMMANDS`). A test checks that every listed command
   and alias is handled there and is not passed through raw.
5. **No server search.** Command arguments use the people and channels the
   board already holds. There is no `LIST` or `WHO` request while you type.
