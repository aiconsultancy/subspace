# Intent: IRC conversation header and member column

After the Slack-style conversation shipped, the owner found the open channel
awkward: nothing separated the header from the transcript, which scrolled
under the topic with lines cut mid-way, the panel bar repeated the channel
as `Channel <network>`, and the member list started below the header while
its toggle sat in the header above, so it looked boxed in and cut off.

The goal is one header row with a divider, a transcript that starts under
it, and a member list that is its own full-height column beside the
conversation, the structure the dock phase keeps (conversation panels and a
right member dock).
