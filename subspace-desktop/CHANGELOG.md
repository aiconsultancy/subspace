# Changelog

Release Please adds an entry for each release of the Subspace App, from the
conventional commits that touch `subspace-desktop/`. See
[Releases](docs/ways-of-working.md#releases).

## [0.9.0](https://github.com/aiconsultancy/subspace/compare/subspace-desktop-v0.8.0...subspace-desktop-v0.9.0) (2026-11-17)


### ⚠ BREAKING CHANGES

* **framework:** Framework crates are imported through subspace_framework; subspace-desktop/framework is gone. make check is repaired in the next commit.

### Features

* **actions:** show per-repository loading and empty states ([72d68ea](https://github.com/aiconsultancy/subspace/commit/72d68ea38030fca622d06e5a7ad0f6a7d8081a34))
* **desktop:** set Subspace's OAuth App and issue repository at build time ([d7deeee](https://github.com/aiconsultancy/subspace/commit/d7deeee33c7ab95c82c5bce6109ec7837b8e3ecc))
* **irc-client:** report nickname metadata status ([#70](https://github.com/aiconsultancy/subspace/issues/70)) ([15492c6](https://github.com/aiconsultancy/subspace/commit/15492c608765392f1e1995f9705e8cdddf526e08))
* **issues:** give the list and issue page more space ([cb31f20](https://github.com/aiconsultancy/subspace/commit/cb31f20cf3de8d35cb781ab0656b2114fd21ed1e))
* **issues:** group terminal states in one workflow stop ([b63dde2](https://github.com/aiconsultancy/subspace/commit/b63dde26f431c240490fc51d867dda1f79f5df42))
* **issues:** preview text and Markdown attachments ([11ab599](https://github.com/aiconsultancy/subspace/commit/11ab59947fea6516677d4477aeeb345a77289621))
* **subspace:** show the Framework version in About ([728fbe2](https://github.com/aiconsultancy/subspace/commit/728fbe2c63f80d5f493ad56e5099157e6310caf6))


### Bug Fixes

* **capture:** route actions and keys past focused composers; align Channels issue help ([#64](https://github.com/aiconsultancy/subspace/issues/64)) ([ed7ee3a](https://github.com/aiconsultancy/subspace/commit/ed7ee3ac087efdb660807037db6654a44b7fe725))
* **channels:** send larger drafts on multiline connections ([60209e7](https://github.com/aiconsultancy/subspace/commit/60209e76eed0373731d4e722412a483088ff2de1))
* **issues:** supply the app's Linear workspace to issue-link recognition ([ce89fb2](https://github.com/aiconsultancy/subspace/commit/ce89fb2bd15b52467b7c942d813a935d7ff21547))
* **update:** require credentials for the selected account ([f323709](https://github.com/aiconsultancy/subspace/commit/f323709c32bca58a8a7d22b404ee6ec37b8906cb))
* **update:** respect the selected GitHub account ([743dad2](https://github.com/aiconsultancy/subspace/commit/743dad2d512a3550857f84c7f06323e5e99e6a18))


### Code Refactoring

* **framework:** split the Framework into its own workspace and crate ([6acbeb6](https://github.com/aiconsultancy/subspace/commit/6acbeb6fd46326e6c94a156e46f60a8e29bd32b6))

## 0.8.0 (2026-11-16)

The last release before Release Please. It and earlier releases are described
in their [GitHub releases](https://github.com/aiconsultancy/subspace/releases/tag/subspace-desktop-v0.8.0).
