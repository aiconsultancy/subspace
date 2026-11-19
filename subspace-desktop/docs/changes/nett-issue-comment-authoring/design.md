# Comment composer design

[Intent](intent.md) · [Spec](spec.md) · [Tasks](tasks.md) · [Delivery](delivery.md)

The primary task is joining the discussion on the open issue or tracker. Keep
the existing timeline, Comments/All control and detail layout. Put a compact
multiline composer after the timeline's paging controls, aligned to its content
spine. Compose the existing shared input/buttons/tokens; add an explicit key
policy to the shared input if needed, keeping IRC's send binding intact.
No Properties, Record, header/workflow or app-shell redesign belongs here.

NETT's `NewCommentForm`, `ThreadReplyArea` and compact Markdown editor are the
interaction reference ([pinned sources](spec.md#1-evidence-and-source-boundary)).
Subspace keeps its own Cupertino Dark default and shared control geometry.
Use a visible New comment label and “Leave a comment” placeholder; a reply
labels the root being answered. The action row contains Attach image… (PR B)
and Comment/Reply, with a visible modifier+Enter hint. Enter and Shift+Enter
insert a newline. A quiet Reply button sits below an eligible root's thread;
its inline composer uses that thread's indentation and opens with focus.

The hub owns text, intent IDs and attachment bytes across issue navigation.
The board owns retained editor selection/caret, focus handles and composition.
Domain work never runs from render; blocking transport/decoding runs off the
UI thread, guarded by scope generation and draft identity. Board UI sees only
safe attachment summaries, never upload or grant URLs. Reusable presentation
stays in board-kit; NETT write semantics stay in `clients/nett`.

The composer grows within the existing detail column and allows long text to
wrap without expanding the window. Attachment rows wrap filename/status and
keep Retry/Remove reachable. Use existing semantic spacing, text, focus and
status tokens, no literal palette or isolated geometry. Keep one clear submit
action and an inline reason whenever it is held. An empty composer, missing
permission and an unavailable profile are distinct states.

Posting freezes text/intake and shows Posting…. Failure puts a nearby named
notice in focus and keeps the draft. A clear refusal offers Retry when its
cause is recoverable. An uncertain post says “We couldn't confirm whether your
comment was posted. Retry won't post it twice.” and offers Retry and Discard.
Retry reuses the frozen body, parent and UUID. Exact DraftAlreadyUsed confirms
the previous save and refreshes the discussion; any other retry refusal stays
unclear and adds Refresh discussion. Discard warns that the comment may already
be posted and allows a fresh draft. Attachment progress and failures are individual rows. Ready
requires verified completion; 100% transferred is still Checking. Remove
cancels only that attachment's upload and cannot remove a saved comment image.

Successful roots clear and refocus the root editor; replies close and restore
their Reply trigger. Background completion never steals focus from another
issue. Escape dismisses the topmost surface, or leaves the editor without
discarding its draft. All actions have keyboard access, visible focus and
accessible names. Progress and errors use text as well as colour.

The [evidence matrix](spec.md#11-delivery-and-evidence) supplies empty/draft,
posting, failed/uncertain, reply, uploading/failed attachment and narrow frames.
Compare them with NETT's desktop composition and the assigned base, and verify
focus and draft lifetime through headless interactions. Source review covers
the contract; real-window screenshots and human live acceptance remain future
delivery work. Review against both GPUI Kit guides before each UI publish.
