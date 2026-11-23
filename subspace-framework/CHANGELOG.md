# Changelog

Release Please adds an entry for each release of the Subspace Framework, from
the conventional commits that touch `subspace-framework/`. See
[Publishing the Framework](docs/publishing.md).

## [0.3.0](https://github.com/aiconsultancy/subspace/compare/subspace-framework-v0.2.0...subspace-framework-v0.3.0) (2026-11-23)


### ⚠ BREAKING CHANGES

* **desktop:** the MCP and CLI tools board_lab_* are renamed space_lab_*, and the desktop product's board-lab Cargo feature is renamed space-lab.
* **framework:** ProductBuild has a new public field, fonts. Code that builds ProductBuild as a struct literal must set it; `fonts: &[]` keeps today's behaviour.

### Features

* **auth:** sign in to WorkOS with device authorisation ([4544de5](https://github.com/aiconsultancy/subspace/commit/4544de5c670707619ee3d0795a0ca6c07f95d437))
* **desktop:** build Fleet Console in, turn Actions on, and add a live fleet fixture ([71bc10a](https://github.com/aiconsultancy/subspace/commit/71bc10ac07fed1798c13569c644b5f0b21399f95))
* **desktop:** open new installs in Twilight Two ([a3c9ca9](https://github.com/aiconsultancy/subspace/commit/a3c9ca9cfec0e41379012da308e5cec319969b1e))
* **desktop:** put every Space in one workspace shell, with Twilight Two and embedded fonts ([e9c7dd3](https://github.com/aiconsultancy/subspace/commit/e9c7dd36a02cf86447307b647ee0afd0537a487d))
* **framework:** add a resize step to evidence captures ([34d0f58](https://github.com/aiconsultancy/subspace/commit/34d0f5820c093ca25dc333700c15a1a28e2ce054))
* **framework:** add the Icons setting with a shared agent icon helper ([46dda59](https://github.com/aiconsultancy/subspace/commit/46dda59aa7b70f6152b7b1227ee6461caa849b8b))
* **framework:** add the workspace shell, Twilight Two and embedded fonts ([b06db15](https://github.com/aiconsultancy/subspace/commit/b06db1533cd2c2b3732ab7da293a183606ba9b75))
* **framework:** agent avatars, pill hues and a crew meter ([bb91389](https://github.com/aiconsultancy/subspace/commit/bb91389e99a45e6cbdf3b31dad5f2207f3500355))
* **framework:** caption rows inside a shared list group ([cef85fa](https://github.com/aiconsultancy/subspace/commit/cef85fa65cc9cc48580298837b68a3bb5201b8fe))
* **framework:** inspector tabs, list captions and two contract fields ([4de180d](https://github.com/aiconsultancy/subspace/commit/4de180d1712a9f1892faccbe021efdc40b32bf97))
* **framework:** optional tabs in the workspace inspector ([45baadd](https://github.com/aiconsultancy/subspace/commit/45baaddbf241cbee5af174e85c89a63f8e4b519d))
* **framework:** ready for agents and check counts in contract summaries ([44c3883](https://github.com/aiconsultancy/subspace/commit/44c38839de664cc5391e8b6bb88672b7d3cb460d))
* **framework:** record a product's theme for a new install ([1f27389](https://github.com/aiconsultancy/subspace/commit/1f27389237b6078077e3c4bdf11f3ca52215d89d))
* **irc:** a navigator by fleet, request lines that settle, and /ask ([329f872](https://github.com/aiconsultancy/subspace/commit/329f8729a803438a5b879acf0ef179c9557de6cf))
* **irc:** agent identity from the API, agent avatars and people-only direct messages ([8cfc406](https://github.com/aiconsultancy/subspace/commit/8cfc406810362939bc4ef1051fdc29103f88f937))
* **irc:** live fleet state in Channels, one navigator group per fleet, and carried review fixes ([5ee62f7](https://github.com/aiconsultancy/subspace/commit/5ee62f70e0823586e69c589dde23416f8ca759d0))
* NETT build of the desktop ([4fed6ed](https://github.com/aiconsultancy/subspace/commit/4fed6ede6a0157362d71049bb0b851c24dbde1fc))


### Bug Fixes

* close the findings of [#32](https://github.com/aiconsultancy/subspace/issues/32)'s first review ([b2c9e69](https://github.com/aiconsultancy/subspace/commit/b2c9e697bba9b885ca0547e4a3fc98c40560a917))
* close the review findings [#29](https://github.com/aiconsultancy/subspace/issues/29) left open ([bef3699](https://github.com/aiconsultancy/subspace/commit/bef36991ec2925347f609e1f6d34b0a739104a9f))
* **fleet:** run the Fleet API client in the app and fix what it showed ([c7dbdc7](https://github.com/aiconsultancy/subspace/commit/c7dbdc70d6d0dcda011e2d9a4886b2c37558ca44))
* **framework:** keep each navigator's row order ([8ba28e4](https://github.com/aiconsultancy/subspace/commit/8ba28e487d6f86f4536e5850eebf9cd59d8a620f))
* **framework:** keep navigator list keys inside the tree and clip yielding docks ([970f8a3](https://github.com/aiconsultancy/subspace/commit/970f8a3851c63d7723a066dc7e1048e282b4ff57))
* **framework:** let workspace docks yield and operate navigator rows from the keyboard ([f69a790](https://github.com/aiconsultancy/subspace/commit/f69a7905f6b01c4526546fb98f4d12d86c643f9a))
* **framework:** name Add a Space items, keep a restart's Space and hide dock switches under zoomed Help ([3ff8d62](https://github.com/aiconsultancy/subspace/commit/3ff8d6280ea0bbd395456124b02c9abdaea11e99))
* **framework:** pin one verified work credential for a Copilot operation ([fc3a41f](https://github.com/aiconsultancy/subspace/commit/fc3a41f180683ff6d158e8e4bed9528a0b363d34))
* **framework:** read one run attempt's Actions jobs ([c39979a](https://github.com/aiconsultancy/subspace/commit/c39979afdadcd342005f8acee9c772b3a25e7fe8))
* **framework:** refuse a nonzero attempt in the default attempt-jobs read ([49e6cc1](https://github.com/aiconsultancy/subspace/commit/49e6cc19c6eeed78d7e8142eba2edb64cd252eb2))
* **framework:** show every Settings › Application row above the Spaces ([d057b42](https://github.com/aiconsultancy/subspace/commit/d057b425be2f1c9134f6197b301461f049ff17be))
* **framework:** show the selected tab by its surface, with no accent edge ([f8301c9](https://github.com/aiconsultancy/subspace/commit/f8301c90d2752843905840c1484a6b13938a6b0c))
* **github-client:** count a 403 with Retry-After as a rate limit ([0d23322](https://github.com/aiconsultancy/subspace/commit/0d2332249529638aa602ac715f1321733d91cdd0))
* **github-client:** keep an ordinary 403 as an access refusal ([2465e93](https://github.com/aiconsultancy/subspace/commit/2465e93c8c19d631a040d20444a1949f019d9e53))
* scope pre-push checks and clarify PR reviews ([14ac1cb](https://github.com/aiconsultancy/subspace/commit/14ac1cbe95b98e4af796ab316c2dba20d8b05287))
* scope pre-push checks by changed project ([ae84f32](https://github.com/aiconsultancy/subspace/commit/ae84f328dde791cff4be765ed98a7b3905cbe01c))


### Reverts

* **github:** move the reference access changes to their own pull request ([deeaaef](https://github.com/aiconsultancy/subspace/commit/deeaaefbb84a5a76c4090f128b1d17e5657d7184))


### Code Refactoring

* **desktop:** rename Board Lab to Space Lab ([f3c25d0](https://github.com/aiconsultancy/subspace/commit/f3c25d0a8ffc9f2f3da83c010f4ec3847e1c20c1))

## [0.2.0](https://github.com/aiconsultancy/subspace/compare/subspace-framework-v0.1.0...subspace-framework-v0.2.0) (2026-11-17)


### ⚠ BREAKING CHANGES

* **framework:** fill the Framework version in without ProductBuild

### Features

* **framework:** publish from CI when a release tag is pushed ([751cee2](https://github.com/aiconsultancy/subspace/commit/751cee2025cfcab42174f74e6f918b533ef3bca2))
* **framework:** show the Framework version in About ([4db2b73](https://github.com/aiconsultancy/subspace/commit/4db2b73900b30bc3ddd0f14168bbf016f849d9e6))


### Bug Fixes

* **framework:** package one checkout at a time ([351b265](https://github.com/aiconsultancy/subspace/commit/351b265f2f0493566402d34fa64a623a7f44e027))
* **framework:** package when the build cache has pruned the target folder ([04f3352](https://github.com/aiconsultancy/subspace/commit/04f3352651a8d111558652e7258512d4240ba0a0))
* **framework:** report the app's version from the mcp command ([6d371da](https://github.com/aiconsultancy/subspace/commit/6d371daade0d4d31bcddf7bd7ecf59d7ed81a681))


### Code Refactoring

* **framework:** fill the Framework version in without ProductBuild ([50bd687](https://github.com/aiconsultancy/subspace/commit/50bd6879c3fae5183f29f2ef1dcd127c59edbf2b))

## 0.1.0 (2026-11-17)

The first release, published to crates.io by hand.
