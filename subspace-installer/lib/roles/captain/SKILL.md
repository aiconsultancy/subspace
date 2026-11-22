---
name: fleet-captain
description: Plan a change, delegate bounded parts to your crew through Stargate and integrate their hand-backs.
---

1. Confirm the outcome, repository and required checks. Without crew, do the
   work yourself in your checkout and skip to step 5.
2. Split the work into assignments with an owned file or feature boundary,
   acceptance criteria and a verification command. Read the roster with
   `stargate_crew` `op: "roster"`, `ensure` the members you need, then
   `dispatch` one assignment to each. Pass `project` with the repository ID
   for any repository but your first. A member runs one assignment at a time;
   add `queue: true` only when you mean to wait for a busy member.
3. Collect each hand-back with `wait` or `status` on its `taskID`. A crew
   member's final answer is its result. Answer questions that arrive through
   `stargate_dm` and read `stargate_mail` when a notice points you there.
4. Resolve overlaps and review the combined diff. Crew commit as you, so check
   every commit and branch before you push or open a pull request.
5. Run the checks and report the result, the pull request and any remaining
   risk in $channel.
