# Web Space errors and fixes

Exact messages from the app (`spaces/web/src/`, `clients/github/src/bound/`)
and from `scripts/package-web-space.py`, which prints the same texts after
`error: `. Paths are relative to `subspace-desktop/`.

## Package and manifest

| Message | Fix |
| --- | --- |
| `The board root needs a regular, readable index.html. Select the built app folder.` | Select the built output folder, not the project root; make `index.html` a regular file at its root. |
| `The board root needs a regular, readable subspace-board.json.` | Add the manifest beside `index.html` (for Vite, in `public/`). |
| `subspace-board.json: expected name to be text of 1 to 60 characters.` | Add or shorten `name`. `version` has the same rule with 40. |
| `subspace-board.json: invalid JSON (…).` | Fix the JSON; no comments, trailing commas or byte-order mark. |
| `subspace-board.json: settings[N]: …` | Fix setting N as the message says; rules in the reference's [Declare an app](../../../../docs/web-spaces-reference.md#declare-an-app). |
| `Unknown permission "…" ignored.` (warning) | Use only `notifications`, `settings`, `badge`, `context`, `secrets`, `issues`. |
| `subspace-board.json is larger than 256 KiB.` | Move data out of the manifest. |
| `The package contains an unsafe or unsupported filename. Use portable ASCII names without .git or device names.` | Rename non-ASCII names, names ending in `.` or space, `CON`/`NUL`/`COM1`…; remove `.git`. |
| `The package has duplicate or platform-colliding paths.` | Rename files that differ only by case. |
| `Package links and special files are not supported.` | Replace symbolic links with real files. |
| `Package files must be regular files, not links or special files.` | A Local folder has a hard-linked file (some package managers make them); copy it, or install the ZIP instead. |
| `A package file exceeds 32 MiB.` / `Package expansion exceeds 256 MiB.` / `Package contains more than 10,000 entries.` / `ZIP exceeds 64 MiB.` | Ship less: drop source maps, raw media, `node_modules`. |
| `ZIP must contain a board at its root or in one enclosing directory.` | Zip the folder's contents, or one folder holding them; the script does this. |
| `Use a regular ZIP with at most 10,000 entries, without ZIP64 or split disks.` | Rebuild the ZIP with the script. |
| `Select an app folder outside the managed board store.` | Choose your own folder, not one inside Subspace's private store. |
| `Write the ZIP outside the web Space folder.` (script only) | Pass an output path outside the folder. |

## Install form and GitHub

| Message | Fix |
| --- | --- |
| `Use a board ID starting with a lower-case letter, followed by letters, digits or hyphens, up to 40 characters.` | Fix the **Board ID**. |
| `This board ID already exists. Edit that board or choose another ID.` | Use **Edit… > Replace source…** on it, or another ID. |
| `A built-in board already uses this ID. Choose another ID.` | Choose an ID that no built-in Space uses. |
| `Sign in to GitHub with an eligible organization account.` | Select a GitHub account in Settings. |
| `This account has no verified active organization membership. Sign in with an eligible account.` | Use an account that is an active member of the repository's organization. Personal repositories never work: use a ZIP. |
| `This account has no verified read access to this organization repository.` | Grant the account read access, or use another account. |
| `Use the repository root URL and enter its ref and board directory separately.` | Enter `owner/repository`; put the branch in **Ref** and the path in **Board directory**. |
| `Select an explicit branch, tag or full commit SHA, not a revision expression.` | **Ref**: blank, `main`, `tag:v1.0` or `commit:<40-hex SHA>`. |
| `GitHub board directory "…": The directory was not found in the repository archive.` | Check the path from the repository root at that ref; commit the built folder. |
| `Select a published release with an eligible named ZIP asset.` | Publish the release (not a draft); name the asset `*.zip`; turn on **Include prereleases** for a prerelease. |
| `This release does not contain the exact selected ZIP asset.` | Keep the asset name identical across releases. |

## Running

| Symptom or message | Fix |
| --- | --- |
| `This app cannot open.` in the Space's settings | The installed manifest is invalid; run the script with `--check`. |
| A blank page | A blocked inline script or remote script (CSP). Move code to files. |
| `window.subspace` is undefined | It is a website Space, or the folder has no manifest; or the code ran in a browser. Feature-detect it. |
| `Permission denied. Allow this action in the board's Subspace settings.` | Request the permission in the manifest, then allow it in **Settings > Spaces > [Space name]** and save. |
| `Notifications are off. Choose a notification channel in Subspace Settings.` | Turn on a notification channel for the Space. |
| `Too many notifications. Wait five seconds.` | Send at most one notification per five seconds. |
| `Resolve at most 50 distinct issue identifiers.` | Split `issues.resolve` calls into batches of 50. |
| `Use issue identifiers of at most 32 characters, without control characters.` | Pass identifiers such as `ABC-827` as strings. |
| `No Space that answers issue requests recognises this issue identifier.` | Check `issues.scope()`: no Space answers, the user is signed out, or the identifier's project key is not in `teamKeys`. |
| `The app manifest changed. Restart Subspace, then review its permissions in Settings.` | Restart and allow the permissions again. |
| A fetch fails | Use HTTPS (or loopback HTTP) and make the server allow the app origin (`subspace://<id>` on macOS, `https://subspace.<id>` on Windows). |
