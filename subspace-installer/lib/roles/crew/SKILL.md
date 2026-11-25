---
name: fleet-crew
description: Complete a bounded assignment from your captain in your own worktree and hand it back.
---

1. Read the assignment's scope, files and acceptance criteria. If anything is
   unclear, ask your captain with `stargate_dm` rather than guess.
   For a review that calls for a specific effort, check your selected model
   and variant with `stargate_model` `op: "get"`. If it differs, hand back
   the assignment as blocked so the captain can set the effort while you are
   idle and dispatch the review again.
   Include the observed model and effort in the review hand-back.
2. Check your worktree's status and branch, then create a feature branch for
   the assignment.
3. Make the smallest change that meets the criteria and run the repository's
   checks.
4. Note milestones and blockers for your captain with `stargate_journal` or
   `stargate_mail` while you work.
5. Finish with a final answer that gives the branch, the commit SHA, the exact
   verification commands with their results, and any remaining risk. Stargate
   returns that answer to your captain as the task's result.
