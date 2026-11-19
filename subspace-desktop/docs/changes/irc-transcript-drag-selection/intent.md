# Intent: drag selection across the IRC transcript

This is the second part of the Channels interaction improvements. Part A (hover,
Shift+Return, the members panel, removing conversations) merged as
aiconsultancy/subspace#8.

As in Slack, a drag from any point in a conversation to any other should
select everything between. Copy should then give the text in reading order.

Before this change, only Markdown messages could be selected across rows.
Plain and mIRC-coded messages and server lines used GPUI's `InteractiveText`,
which takes no part in a selection. Membership lines used Kit's
`SelectableText`, which copied before everything else. Earlier analysis
expected two Kit obstacles: an arbitrary copy order, and long drags past rows
that `MessageScroller` unmounts.

Kirk's route, chosen by Jack, is **option C**: no upstream Kit PR and no
local Kit patch, only what GPUI Kit 0.9.1 offers. Jac suggested the
renderer the PR detail panel uses, Kit's selectable `TextView`. Step 1
proves how Kit orders copied text (see [findings](findings.md)). Step 2 puts
every row's text through that `TextView`.
