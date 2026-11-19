# Intent: board-to-board contracts

Back to the [change records](../README.md).

## The owner's direction

> We want a defined way for board to board communication. A lot of what we
> have is board to framework, so like sending to chat is not board to board
> communication; that is pushing data out from a board to what is a framework
> feature and indeed vice versa, and that's quite an easy simple
> architecture: boards expose tools to write data in, and they're given the
> hooks to send data back to the platform. So what we might be missing is
> actual board-to-board registration, like I want to register for issues. I'm
> trying to think how this connectivity can work between the IRC and the
> issues board when we might have an issue board disabled. So they both have
> to be aware that each other exists, or an interface for that exists. It's
> the interface: they have to be aware that there's an interface, and that
> interface is effectively a contract, and they're saying for this contract,
> here's our receiver, and also vice versa, we want to send these things. So
> I think probably the best way for that is to have an observer publish and
> subscribe, but through a broker or an observer pattern, so there's no hard
> reference, a weak reference.

Later in the same session:

- The first real case is Issues and IRC. The agent building the Issues board
  (the issue workspace implementation) handed over everything that crosses boards: the IRC side of
  issue references (slice 4), the reference tooltip (slice 8), the batch
  resolver (slice 12), the reference and avatar hook designs, and the
  cross-board part of avatar support. The mechanism must fit that case
  directly, with an `issues` contract that IRC uses without depending on
  `clients/nett` or the Issues board, and that is unavailable when the Issues
  board is disabled.
- Decide where the pieces of `framework/services/issues` belong under this
  design. The Issues agent holds the move until this record says.
- "We're picking up this work from another agent. Roll this into the
  delivery plan." [tasks.md](tasks.md) is the delivery plan for all
  board-to-board work, in ordered phases.

## Why

Before this change, nothing let a board tell another that something happened
or ask it a question. The paths that existed were built for one job each,
mostly between a board and the Framework, and the one board-to-board path
(NETT's process-wide hub registry, polled by version) was implicit: nothing
declared who published or read it, and no test checked it.

## Scope of this change

Phases 1 and 2 of the [delivery plan](tasks.md): the mechanism (contracts
crate, declarations at registration, the weak-reference broker, test support,
docs and the checklist) and the `issues` contract definition with the
placement decision. No board adopts a contract here.
