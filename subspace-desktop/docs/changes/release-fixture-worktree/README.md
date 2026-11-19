# Isolate macOS release fixtures

The macOS release fixture creates a temporary Git repository. A pre-push hook
in a linked worktree can pass the calling worktree's Git directory and index to
that script. A developer's global signing configuration can also make the
synthetic commit invoke their signing program.

The fixture clears Git's worktree-local variables before creating or reading its
repository. The synthetic commit disables signing for that command. The
prerelease Python tests already isolate their repositories and configuration.

The script suite launches the macOS fixture from a temporary linked worktree
with the hook variables set and global signing enabled. It checks that the
fixture passes and leaves the calling worktree's commit and working tree intact.
