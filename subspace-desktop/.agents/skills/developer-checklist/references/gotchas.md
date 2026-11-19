# Gotchas

Failures seen in this repository and what fixed them.

- **Pre-push hook fails with "files were modified by this hook".** Another
  process edited the working tree during the push. Push when no one else is
  editing, or verify in a clean checkout: run `make check` and
  `mise run quality` there by hand. Do not run the hook from a separate
  worktree: Git exports `GIT_DIR` and the index to hooks, and tests that run
  Git can then write to the real repository.
- **Rejected push (`fetch first` / non-fast-forward).** Others push to
  `feat/subspace-desktop` often. Fetch, `git rebase` onto the remote branch,
  re-run the affected checks, push again. Never force-push the shared branch.
- **`gh pr merge` deleted the head branch or switched the checkout.** The
  repository deletes head branches on merge, and `gh pr merge` checks out the
  base branch when the head branch is checked out. Merge integration work
  through a temporary branch that is not checked out. Even then, merging PR
  #51 from a temporary branch at the same commit as `feat/subspace-desktop`
  deleted both branches on GitHub and moved the checkout to `main`. After
  every merge, run `git branch --show-current` and `git ls-remote` for the
  feature branch. If it is gone, check out the local branch (its files match
  `main`), fast-forward it to `main` and push it again.
- **Signing or notarisation fails over SSH** (`errSecInternalComponent`, "No
  Keychain password item"). `make package-macos` and `make release-macos`
  move into the desktop session by themselves (`scripts/desktop-session.sh`);
  the Mac must be logged in at its desktop. `launchctl managername` prints
  `Aqua` there.
- **"No space left on device".** The shared build cache grows past 100 GB.
  With no build running, `mise exec -- mbx clean` in `subspace-desktop` and
  `mbx gc`; never delete files inside the cache by hand (stale fingerprints
  break the next build).
- **zsh scripting.** zsh does not word-split `$VAR`, aborts on unmatched
  globs, ties `path` to `PATH`, and treats `$sha:path` as a modifier. Use
  arrays, `${var}`, other variable names, or bash.
- **A screenshot showed the real GitHub account.** A hand-written capture
  script set only `HOME`; `open` passed the caller's `XDG_CONFIG_HOME`, so the
  app read the real `gh` accounts and the token from the Keychain. Capture
  only with `make capture`, which starts the app from an empty environment
  with fixture servers.
- **"Duplicate a11y node id" panic in a debug build.** Two elements in one
  frame share an element ID. GPUI checks this only while an accessibility
  client (VoiceOver, cua-driver) is connected, so headless tests miss it.
  The usual cause is one retained state (`InputState`, `SliderState`) drawn
  twice: GPUI Kit derives the input's and slider's IDs from their state.
  Give each drawn instance its own state, and open the changed page with
  `make capture` to check.
- **Builds stall downloading the Copilot CLI.** Set
  `COPILOT_SKIP_CLI_DOWNLOAD=1` for checks and pushes.
