---
name: fleet-captain
description: Plan a change, delegate bounded parts to your crew through Stargate and integrate their hand-backs.
---

1. Use the assignment to establish the outcome, repository and required
   checks. Without crew, do the work in your checkout and run the required
   checks. For a delivery assignment, use the publishing route in your
   officer instructions. Report the result and finish the solo assignment.
2. Split the work into assignments with an owned file or feature boundary,
   acceptance criteria and a verification command. Read the roster with
   `stargate_crew` `op: "roster"`, `ensure` the members you need, then
   `dispatch` one assignment to each. Pass `project` with the repository ID
   for any repository but your first. A member runs one assignment at a time;
   add `queue: true` only when you mean to wait for a busy member.
   For a review, use `stargate_model` with `op: "inspect"` on the selected
   worker and project. Choose `medium`, `high` or `xhigh` to match the task
   or the repository's review gate. While the worker is idle, call
   `op: "effort"` with that `target`, `variant` and the same `project`,
   then inspect again before dispatching. If the model does not offer the
   required variant, choose another worker that does.
3. Collect each hand-back with `wait` or `status` on its `taskID`. A crew
   member's final answer is its result. Answer questions that arrive through
   `stargate_dm` and read `stargate_mail` when a notice points you there.
   Finish a review or investigation by reporting the result to the requester.
   Continue with steps 4 and 5 for changes assigned for delivery.
$completion

## Provider quota

Follow the usage guard guidance in your officer instructions when a ship
notice names a provider.

Include the `UPL-…` reference and file name when assigning work that needs
a chat upload. Use `subspace_read_chat_upload` if you received the reference
as text.
