# Tasks: IRC board

- [x] `spaces/irc` crate, registered in the Subspace product after Teams
      (default on in debug builds, off in release builds), with its icon,
      help guide, product descriptor, interface script and docs entries.
- [x] Process-wide hub: runtime started in `start_background`, 50 ms event
      drain, stopped with the board.
- [x] Pure state reducer: networks, buffers (2,000-line cap), topics, members,
      typing expiry, read markers, unread and mention counts, history
      prepending and `msgid` de-duplication, local failure lines.
- [x] Rows grouped by network with connection state in text and tone;
      server, channels, queries; counts; bold mentions; badge and status.
- [x] Transcript in `MessageScroller` with stable identities, compact lines,
      nick tones, mIRC formatting and colours as theme tones, links, actions,
      notices, condensed membership runs, day separators, highlights.
- [x] Topic line, member column (toggle), typing line, server facts panel.
- [x] Composer per buffer: input commands, throttled typing notices, Up/Down
      recall and Tab nick completion (new `ChatInput` plain-text and
      recall-key options).
- [x] Recent history on open, **Load older messages**, `MARKREAD` on read.
- [x] Toolbar: Connect/Disconnect, Join… dialog, Members, More (Leave
      channel, Close private messages).
- [x] Notifications for mentions and private messages in buffers not in view;
      opening one selects the buffer. `/query` shows the conversation.
- [x] Empty state with the configuration example and Configure action.
- [x] Reducer, row, condensing, colour, styling, completion, recall, typing
      and render tests.
- [x] Screenshots against the local Ergo server in One Dark, Cupertino Light
      and Cupertino Dark.
- [x] Review fixes: casemapping-aware buffer identity and member spellings,
      history answered without `draft/chathistory`, history cap
      (`LINE_CAP` plus loaded pages, 10,000 at most) and in-page `msgid`
      de-duplication, cancellable connects with 15 s TCP and TLS timeouts,
      Join only while connected, "Not connected" lines for undeliverable
      commands, user mode sentences.
- [ ] Automatic history loading when the transcript reaches the top (the
      pinned `MessageScrollerState` exposes no top-of-list signal; the board
      uses an explicit **Load older messages** row).
- [ ] Text selection within a transcript line.
- [ ] Per-channel notification choices, ignore list, channel browser.
