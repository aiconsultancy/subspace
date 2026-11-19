# Intent: IRC issue references

Back to the [change records](../README.md).

Phase 4 of the [board-contracts plan](../board-contracts/tasks.md), which the
owner started on 15 November 2026 with phase 3: IRC adopts the `issues`
contract. It carries the IRC side of NETT issue workspace slices 4
(references and opening an issue), 8 (the reference tooltip), 9 (transcript
keyboard focus) and 12 (the batch resolver), handed over by the issue workspace implementation
([NETT issue workspace tasks](../nett-issue-workspace/tasks.md)).

IRC must not depend on `clients/nett`, `framework/services/issues` or the
Issues board: it recognises references with the contract's recogniser, asks
whichever board answers the contract, and shows plain text when none does.
