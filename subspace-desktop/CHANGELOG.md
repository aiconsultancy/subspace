# Changelog

Release Please adds an entry for each release of the Subspace App, from the
conventional commits that touch `subspace-desktop/`. See
[Releases](docs/ways-of-working.md#releases).

## [0.11.0](https://github.com/aiconsultancy/subspace/compare/subspace-desktop-v0.10.0...subspace-desktop-v0.11.0) (2026-11-22)


### ⚠ BREAKING CHANGES

* **desktop:** the MCP and CLI tools board_lab_* are renamed space_lab_*, and the desktop product's board-lab Cargo feature is renamed space-lab.

### Features

* **client-subspace-api:** replay a NETT move confirmation after a lost reply ([6b21058](https://github.com/aiconsultancy/subspace/commit/6b21058ba2c60ad51172e41f7d65dbb8f73dd71b))
* **decisions:** add the Decisions Space ([8e5700b](https://github.com/aiconsultancy/subspace/commit/8e5700bb43f3c9c623e60abf8c9ed9e28e1f61d2))
* **desktop:** embed Inter and JetBrains Mono ([e729220](https://github.com/aiconsultancy/subspace/commit/e729220a983a74185c63a6ccf38d910d1d811159))
* **desktop:** move Actions, Copilot Agents, Board Lab and web Spaces onto the workspace shell ([1bf206d](https://github.com/aiconsultancy/subspace/commit/1bf206d71a79dd6e9ffb748492240549883917d8))
* **desktop:** register Decisions and leave unreleased Spaces out of default builds ([9aa581f](https://github.com/aiconsultancy/subspace/commit/9aa581f21520af34e8ace409612c9542b539dbd0))
* **github:** move Pull requests onto the workspace shell ([2950787](https://github.com/aiconsultancy/subspace/commit/2950787096c6a0fc369f2ba985ed50ba0aae8985))
* **irc:** move Channels onto the workspace shell ([6e80af2](https://github.com/aiconsultancy/subspace/commit/6e80af25ec6f7c3c108c216406d3a70e4d1745d8))
* **irc:** show agents' status lines in Channels ([70cff6a](https://github.com/aiconsultancy/subspace/commit/70cff6aea8a2832217023001e2ce54a8fa3f06f5))
* **irc:** show one live status line per request thread ([b455d0e](https://github.com/aiconsultancy/subspace/commit/b455d0eb0e6990fdf51e607d2626b565fbcf06a7))
* **issues:** move Issues onto the workspace shell ([becc0c6](https://github.com/aiconsultancy/subspace/commit/becc0c6e018dc52d1771059647aa28bac8edd8df))
* **issues:** open a replayed move under the number NETT recorded ([d290519](https://github.com/aiconsultancy/subspace/commit/d2905192e85a34f69cd0a10d742ba25c62e9d0ad))


### Bug Fixes

* **client-subspace-api:** announce a capability change whichever read found it ([d2be0cf](https://github.com/aiconsultancy/subspace/commit/d2be0cf60f54c60cc924469f82762129e9f2c6c4))
* **client-subspace-api:** back off reading an unreadable capability list ([4664777](https://github.com/aiconsultancy/subspace/commit/4664777e0f342a9930918e4598cde43449907195))
* **client-subspace-api:** bound a move read-back's wait for a request slot and check each reply's issue ([3389a58](https://github.com/aiconsultancy/subspace/commit/3389a58533240fc74b7d4563574084b90026eeb2))
* **client-subspace-api:** bound a NETT move's read-back and end it on a scope change ([8a9e013](https://github.com/aiconsultancy/subspace/commit/8a9e0138b03236351ea113595323a6e32d2844e0))
* **client-subspace-api:** bound parked uploads and key them by issue ID ([f6c755d](https://github.com/aiconsultancy/subspace/commit/f6c755d42358b2045272d1916277f9a2ce839b18))
* **client-subspace-api:** bound the upload store and keep in-flight bytes ([c5a837f](https://github.com/aiconsultancy/subspace/commit/c5a837f55766524448abc59f79a29ba8926eca35))
* **client-subspace-api:** bring a refused stream back when another read finds access changed ([9259d41](https://github.com/aiconsultancy/subspace/commit/9259d4158ae8bb02c4019a46ed8806ba8a0b8a4f))
* **client-subspace-api:** count only bytes letting go frees, and expire before reopening ([810ad75](https://github.com/aiconsultancy/subspace/commit/810ad753a6cf637a5bc9519747a34c4463a595db))
* **client-subspace-api:** decide once, in the hub, whether a move check replays ([af995bf](https://github.com/aiconsultancy/subspace/commit/af995bf4c62b103bc43b4826efc4af7e0a1058e5))
* **client-subspace-api:** finish a move's Check once its root reads back moved ([91c518f](https://github.com/aiconsultancy/subspace/commit/91c518f0c0269ec7206a0bd667373abbd8f32c57))
* **client-subspace-api:** honour a chat draft's wait before any capability read ([170b0eb](https://github.com/aiconsultancy/subspace/commit/170b0eb2d1c976ea50ecbb2377d8b8e907b929e9))
* **client-subspace-api:** judge a refused stream's restart against the list before its own read ([217b236](https://github.com/aiconsultancy/subspace/commit/217b23665d4e2da90e411ef6cd065d11386ae8ce))
* **client-subspace-api:** keep a NETT move committed when a sub-issue cannot be read ([9d5090a](https://github.com/aiconsultancy/subspace/commit/9d5090a22b1b5c6eb675938763ef78d5bfb6f60e))
* **client-subspace-api:** keep an upload that may be stored until its outcome is known ([963ea63](https://github.com/aiconsultancy/subspace/commit/963ea6307b8844a3f8dd9337a30345f5028014fc))
* **client-subspace-api:** keep standalone uploads in one store by issue ID ([60f5e1c](https://github.com/aiconsultancy/subspace/commit/60f5e1c0e2510f8eea8fc4164455e5bfe65e0a41))
* **client-subspace-api:** leave the loopback flow step out of release builds ([4ce753a](https://github.com/aiconsultancy/subspace/commit/4ce753a1be4fdbbda85d8bcb8645a3e72d703150))
* **client-subspace-api:** name an upload's issue by its ID and cancel what a let-go job holds ([4d087ce](https://github.com/aiconsultancy/subspace/commit/4d087ced1076183cb3513637e8f92359d1716cdb))
* **client-subspace-api:** never record a replayable confirmation without an account ([1c6e619](https://github.com/aiconsultancy/subspace/commit/1c6e619318d4b9f2c92884e539519990c6eb8716))
* **client-subspace-api:** never take an incomplete attachment record as absence ([ed5c37a](https://github.com/aiconsultancy/subspace/commit/ed5c37a6e148eb039eb4707c4fe8d7ae579489c2))
* **client-subspace-api:** never take an unproven upload ID as absence ([d918d56](https://github.com/aiconsultancy/subspace/commit/d918d566c39b4bbebbb814cdfa921dfcf12e348d))
* **client-subspace-api:** replay only what this account sent with replay on, and only to writable teams ([00efa00](https://github.com/aiconsultancy/subspace/commit/00efa00b0f23e4cd2eda8287fa56ac4d740f317b))
* **client-subspace-api:** take an empty upload ID as naming no transfer ([916323c](https://github.com/aiconsultancy/subspace/commit/916323c86e84a7c445899d0d6253dbe52e9820da))
* **client-subspace-api:** take only well-formed fields as proof of another file ([6900908](https://github.com/aiconsultancy/subspace/commit/69009082c73ed3632086862a7a14fa5755effaf4))
* **client-subspace-api:** tie a Check's moved root to the plan by its sub-issues ([0f06cb0](https://github.com/aiconsultancy/subspace/commit/0f06cb04cfc1710e1a0d81bda436df891a27009e))
* **client-subspace-api:** turn the NETT stream on when any read enables updates ([094bab4](https://github.com/aiconsultancy/subspace/commit/094bab4bce5c7b59f9ccea3bc329afc0795974dc))
* **desktop:** build Channels palette offers from the conversation shown ([3b6f02f](https://github.com/aiconsultancy/subspace/commit/3b6f02f7e2d135a463ab7b84eb410927c6a9f251))
* **desktop:** check licences against resolved sources ([1728ca2](https://github.com/aiconsultancy/subspace/commit/1728ca2be648970e984e07c5409e5e8f9c02e1f0))
* **desktop:** complete native release licence notices ([10190ae](https://github.com/aiconsultancy/subspace/commit/10190ae41997fc2cb3395db758e9badbf3ff3090))
* **desktop:** create Space Lab project folders without a start-up race ([6c830f3](https://github.com/aiconsultancy/subspace/commit/6c830f3a4d85190e36fccdba03fbc4cfe45eea39))
* **desktop:** drop the Decisions Space's data when the account changes ([9401c90](https://github.com/aiconsultancy/subspace/commit/9401c9057bf2d197672376adeb3fdf9de4e54b18))
* **desktop:** free Pull requests drafts during other writes and name IPv6 check hosts ([35dcaec](https://github.com/aiconsultancy/subspace/commit/35dcaecd5d3cbcf5c54026f86cdc14fb6f399d62))
* **desktop:** harden release build and archive staging ([e76a3b1](https://github.com/aiconsultancy/subspace/commit/e76a3b1070a260a2e19133dc24b632aab51c255a))
* **desktop:** keep Actions writes, jobs and logs on the run shown ([c8a31e6](https://github.com/aiconsultancy/subspace/commit/c8a31e6f78346f320aa15d76ef2bd764af3f44c6))
* **desktop:** keep Channels toolbar actions on the conversation shown ([d033ec5](https://github.com/aiconsultancy/subspace/commit/d033ec5ace98b8a03296aa59b6d653a67f07c49b))
* **desktop:** keep Copilot Agents tabs, filters, login and log reads current ([68866e5](https://github.com/aiconsultancy/subspace/commit/68866e5b29a050194aaa1473a0de6fbee84ec30e))
* **desktop:** keep Pull requests review drafts to their account and pull request ([23310bc](https://github.com/aiconsultancy/subspace/commit/23310bc843aee9c891d184f66d97e9c2c70d2d75))
* **desktop:** keep the Issues organisation menu and Back origin right ([072c09b](https://github.com/aiconsultancy/subspace/commit/072c09bc3377e4f7f8c25e922ab7213e54e62aca))
* **desktop:** lock release builds and archive publication ([2e86c4c](https://github.com/aiconsultancy/subspace/commit/2e86c4c8fd46ad7460eed5f95be96542a54679d8))
* **desktop:** preserve upstream release licence notices ([c4abc1a](https://github.com/aiconsultancy/subspace/commit/c4abc1a19dce3277ee577c11109af2c12dffc506))
* **desktop:** read Actions jobs for the attempt shown ([b883ad3](https://github.com/aiconsultancy/subspace/commit/b883ad347e31838e28fe16a0de53d24c7c525558))
* **desktop:** read Copilot Agents data with one pinned work credential ([970a4d2](https://github.com/aiconsultancy/subspace/commit/970a4d2c4e9c0c97f40cbd0d780ffbac5186fb34))
* **desktop:** read the board list from saved metadata in the Space Lab interfaces check ([1fb1ef7](https://github.com/aiconsultancy/subspace/commit/1fb1ef71e6643ac550b7e153eb093d19622d52d7))
* **desktop:** read, cache and index Copilot Agents data under one work account ([cfa4ffc](https://github.com/aiconsultancy/subspace/commit/cfa4ffc7efccdad21b2c222a2988c7014ccf8aab))
* **desktop:** reap Cargo after process group cleanup ([28c513b](https://github.com/aiconsultancy/subspace/commit/28c513b5a7baf631ae728793c93ca5f9e73691fc))
* **desktop:** refuse unreleased Spaces in release packages ([4a23741](https://github.com/aiconsultancy/subspace/commit/4a237419433960aa2537648430809ef54570bc90))
* **desktop:** run cargo-about directly when packaging ([28c7829](https://github.com/aiconsultancy/subspace/commit/28c7829dd8861b7be32f9a0b2f01d1c832d6369d))
* **desktop:** ship the embedded fonts' licences in release packages ([8e76e21](https://github.com/aiconsultancy/subspace/commit/8e76e21d822e465a344010d00fe1d702910f0344))
* **desktop:** simplify release packaging safeguards ([4fce7c4](https://github.com/aiconsultancy/subspace/commit/4fce7c4412112df1081e43edd1223c7da6488672))
* **desktop:** split release features on any whitespace and read boards before the target check ([1779b73](https://github.com/aiconsultancy/subspace/commit/1779b73e48b2378afd5d5ee4c4f61ae5239f0722))
* **desktop:** take each Decisions read's account lease from its own client ([8c4d521](https://github.com/aiconsultancy/subspace/commit/8c4d521d1ee1454e740ca00734595f878ba3e684))
* **desktop:** treat EPERM from a finished Cargo group as stopped on macOS ([22f437e](https://github.com/aiconsultancy/subspace/commit/22f437ee4b80da46e6a1b77ff32ed44b2c30a3ab))
* **desktop:** verify native release inputs before packaging ([5eb25ff](https://github.com/aiconsultancy/subspace/commit/5eb25fff943ae93d8ad22a7b1e56602981e6e5e9))
* **desktop:** verify native release licence notices ([ad45e95](https://github.com/aiconsultancy/subspace/commit/ad45e956b6e32929748954c185c36a36576c86b2))
* **desktop:** verify release inputs and recover interrupted packaging ([8c9cc8c](https://github.com/aiconsultancy/subspace/commit/8c9cc8cdc5a34c48155f2d59cf30c1d5e7df68e5))
* **desktop:** verify release licence provenance ([14ba208](https://github.com/aiconsultancy/subspace/commit/14ba208dc349d492fe85e17646843261f8577eac))
* **framework:** name Add a Space items, keep a restart's Space and hide dock switches under zoomed Help ([3ff8d62](https://github.com/aiconsultancy/subspace/commit/3ff8d6280ea0bbd395456124b02c9abdaea11e99))
* **gallery:** correct bundled glyph licence notices ([261b294](https://github.com/aiconsultancy/subspace/commit/261b294ccced652f780ccc06a78f340d2b7f6149))
* **irc:** address review findings on agent status lines ([1f49861](https://github.com/aiconsultancy/subspace/commit/1f49861aad59f68a20e4d3c6481b0d17fea62d9a))
* **irc:** pin each request's trusted account and address review findings ([0c6d4a0](https://github.com/aiconsultancy/subspace/commit/0c6d4a0cba74820ccaf124dcfa83025f03e1153b))
* **irc:** store request bindings and endings, bind from the first answer ([ac76349](https://github.com/aiconsultancy/subspace/commit/ac763493d614cdfb6e6f1a542563dcc7e59fc7ac))
* **irc:** trust a request's progress when its account is the addressed nick ([3ccdff8](https://github.com/aiconsultancy/subspace/commit/3ccdff80817686e665e3563186bc9cefdd03aa4b))
* **issues:** list the sub-issues a NETT move could not confirm ([6d5f13c](https://github.com/aiconsultancy/subspace/commit/6d5f13ce4d236066f3246e532bf7c75e8fc3e9d3))
* **issues:** offer saved attachments under names every platform accepts ([ac81736](https://github.com/aiconsultancy/subspace/commit/ac81736398755fb06ca35c7ad182bfd8e5ed987a))
* **issues:** offer Send again with a warning when checks cannot tell ([637e99e](https://github.com/aiconsultancy/subspace/commit/637e99e5229e477e34e1fc48a97f10c0de66bddb))
* **issues:** open a committed move's root by its stable ID ([caf430b](https://github.com/aiconsultancy/subspace/commit/caf430b8feea9ce19b55e746369b3632f2fffcf1))
* **issues:** read a chosen attachment only up to the upload limit ([49f1540](https://github.com/aiconsultancy/subspace/commit/49f1540ffd2b3246787768055e07126d18ac0a11))
* **issues:** save attachments to Downloads under a plain name ([f56962a](https://github.com/aiconsultancy/subspace/commit/f56962a05e79b879d7d7864bc815a2a1c835426f))
* **issues:** say only what a move's read-back showed for each sub-issue ([0d6db18](https://github.com/aiconsultancy/subspace/commit/0d6db1831cf5547aa628fbaefa8fbf530f2cb080))
* **issues:** scroll a long list of unconfirmed sub-issues and keep Open in view ([e82971e](https://github.com/aiconsultancy/subspace/commit/e82971e8fdc7e9f8e315ccad110cfc72f14f8550))
* **issues:** wrap an upload's status within the Attachments column ([9a4afe0](https://github.com/aiconsultancy/subspace/commit/9a4afe0bc08b592f7c6b838db04f68aad7ffaea3))
* **nett:** keep a capability list only for the account it was read for ([220257d](https://github.com/aiconsultancy/subspace/commit/220257d24a0c695255ce045b68f399bbc68952f0))
* **nett:** keep a committed move committed when a sub-issue moves again ([671b40b](https://github.com/aiconsultancy/subspace/commit/671b40b5da9f7b73e5fc64b9b57a3b8833cbef79))
* **nett:** keep an issue's uploads when it closes ([a37760a](https://github.com/aiconsultancy/subspace/commit/a37760a055b9344740f086c520a8782fbc7b8650))
* **nett:** never take an unreadable record of the transfer as absence ([5aed5e9](https://github.com/aiconsultancy/subspace/commit/5aed5e9c40d0a6ce3b4e3be68e895b7b98439a01))
* **nett:** read the capability list again on reload and once it is old ([14328a2](https://github.com/aiconsultancy/subspace/commit/14328a24f6c1c739da0fce5de01ed2adf8b7a315))
* **nett:** wait at most a minute in all for an upload's completion ([c9bddca](https://github.com/aiconsultancy/subspace/commit/c9bddca340397cf3cfc8814b02c9ddae883937b3))
* scope pre-push checks by changed project ([ae84f32](https://github.com/aiconsultancy/subspace/commit/ae84f328dde791cff4be765ed98a7b3905cbe01c))
* **subspace:** check a WorkOS client ID when a Subspace build launches ([f5c5c5d](https://github.com/aiconsultancy/subspace/commit/f5c5c5d5e4865c9896ff58ffa1363a295f973c27))


### Code Refactoring

* **desktop:** rename Board Lab to Space Lab ([f3c25d0](https://github.com/aiconsultancy/subspace/commit/f3c25d0a8ffc9f2f3da83c010f4ec3847e1c20c1))

## [0.10.0](https://github.com/aiconsultancy/subspace/compare/subspace-desktop-v0.9.0...subspace-desktop-v0.10.0) (2026-11-20)


### ⚠ BREAKING CHANGES

* **issues:** Issues reads Subspace API instead of NETT.

### Features

* **channels:** initiatives, fleets, attachments, PR chips and captain notices ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **client-subspace-api:** add a NETT backend behind the client operations ([61446e5](https://github.com/aiconsultancy/subspace/commit/61446e580d1a4a03559d62fb1f6223a5e61d08fa))
* **client-subspace-api:** read an issue's decisions and one decision in the hub ([e0ab042](https://github.com/aiconsultancy/subspace/commit/e0ab0425d8070ef07250ebf64a5ad3a6670c8363))
* **component-gallery:** add React proof and strict pixel comparison ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **component-gallery:** implement full React inventory and fix review interactions ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **component-gallery:** Linux/React proof (parity incomplete: 0/6 strict) ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **desktop:** configurable identity provider and Subspace API URL ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **desktop:** show dev sign-in in captures of the local stack ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **fleet-console:** add the Fleet Console web Space ([a55b749](https://github.com/aiconsultancy/subspace/commit/a55b7492eb91dff2e7387488d3c40163cb238b5d))
* **fleet-console:** add the Fleet Console web Space ([ebdbc7d](https://github.com/aiconsultancy/subspace/commit/ebdbc7d1a826e466e1b4957fcd46dbf3615b4043))
* **framework:** close a Space from its tab's right-click menu ([263ae6c](https://github.com/aiconsultancy/subspace/commit/263ae6cfbb6eb3b64cf9ef30ed39c6d07f641094))
* **irc:** add protocol acceptance coverage and provenance ([c34397d](https://github.com/aiconsultancy/subspace/commit/c34397d0da99e36477869a2c83f8c11c748f7994))
* **irc:** follow what the organisation serves in Channels ([40b4274](https://github.com/aiconsultancy/subspace/commit/40b42747e710031b9fbc96141bf10b480a782499))
* **irc:** replace the protocol layer with a clean-room implementation ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **issues:** add the Decisions tab and decision panel to issues ([678ddef](https://github.com/aiconsultancy/subspace/commit/678ddefe1da3bb65ce3ea00d6668293edf16a9f5))
* **issues:** Subspace API client and Issues Space replace NETT ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* NETT build of the desktop ([4fed6ed](https://github.com/aiconsultancy/subspace/commit/4fed6ede6a0157362d71049bb0b851c24dbde1fc))
* **prs,actions:** close pull requests, PR navigation and Actions filters ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **subspace:** build a NETT variant that signs in with device authorisation ([6efd70b](https://github.com/aiconsultancy/subspace/commit/6efd70bc9b32573e2febab960f3e185f4fb43b26))
* **web:** add the secrets and issues app permissions ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **web:** give app pages the theme, their secrets and issues ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **web:** give web Space pages the theme, their secrets and issue tiles ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **web:** install the Weather example from the release repository ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **web:** let app pages save secrets of their own ([1134779](https://github.com/aiconsultancy/subspace/commit/113477981e59baf2cd2b91489e898feedb172e74))
* **web:** merge the web Space bridge for Subspace API ([69aac0f](https://github.com/aiconsultancy/subspace/commit/69aac0f6fb94c0cb6bd2648a24ac4b8eb2a35c34))
* **web:** show web Space pages in capture screenshots ([dbba1dd](https://github.com/aiconsultancy/subspace/commit/dbba1dd0cd56fbeabb615e239a594eafc6812291))


### Bug Fixes

* address the review findings for the web bridge, close Space and Fleet Console ([#6](https://github.com/aiconsultancy/subspace/issues/6)) ([aa9fd80](https://github.com/aiconsultancy/subspace/commit/aa9fd805a1f1632866b637ff0f49bceb7dbe3e88))
* **api-client:** preserve sign-out and fixture stream auth ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **build:** isolate desktop stamp Git discovery ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **capture:** start fixtures with macOS Bash ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **channels:** honour initiative channel separators and fleet routes ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **client-subspace-api:** match recovered uploads to their transfer and bound uploads ([2f0c369](https://github.com/aiconsultancy/subspace/commit/2f0c369d59c4fd69e5a5208294bd98a3e445ba9b))
* **client-subspace-api:** re-read decisions from the first page on refresh ([fa7abad](https://github.com/aiconsultancy/subspace/commit/fa7abaddeae46f4bb1938aee78c82d37314af6c9))
* **component-gallery:** align fixture fills and text layout ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **component-gallery:** isolate Linux browser test session ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **desktop:** reload API Spaces after the startup dev sign-in ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **desktop:** require the signing Key Vault name from the environment ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **desktop:** restore Copy link and selectable find matches ([ed91db6](https://github.com/aiconsultancy/subspace/commit/ed91db62679794b01fc0e57c08c5769e88813d25))
* **dev-auth:** advance the generation when sign-in commits ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **dev-auth:** omit unset identity fields ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **irc:** bound open batches and buffered messages ([4709f99](https://github.com/aiconsultancy/subspace/commit/4709f99f2e55c98893971cae9a8cb070dcb9a206))
* **irc:** declare the Rust 1.99 toolchain floor ([2108350](https://github.com/aiconsultancy/subspace/commit/21083500a7bb32d9bc5e085cf9384fa0e1af393b))
* **irc:** document round-two limits and empty tag keys ([1ca363b](https://github.com/aiconsultancy/subspace/commit/1ca363b856bf61ff9a244b717a81b561fbaa5a44))
* **irc:** keep a fleet's own network in loopback capture ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **irc:** keep initiative autojoin off while its fleet is unresolved ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **irc:** rate-limit automatic CTCP replies ([693e688](https://github.com/aiconsultancy/subspace/commit/693e6886cc25819fdc4cc3919b05008edc5d7fd3))
* **irc:** stop exhausted nickname registration attempts ([37ea248](https://github.com/aiconsultancy/subspace/commit/37ea248d5a04b18ba720bfdb7959953a44b03900))
* **irc:** time out blocked socket writes ([267446f](https://github.com/aiconsultancy/subspace/commit/267446f11531657febe8314cc51e9111ee64d3fa))
* **irc:** time out stalled SASL registration ([8639ebd](https://github.com/aiconsultancy/subspace/commit/8639ebd488486789e76bf795960e11286ff95312))
* **issues:** describe decisions in the API's decided-on order ([eeb81f0](https://github.com/aiconsultancy/subspace/commit/eeb81f0460c33f398872a68cdb26e2750b1ac575))
* **issues:** keep keyboard and assistive focus usable in the decision panel ([bab1da6](https://github.com/aiconsultancy/subspace/commit/bab1da6059a2a2a6636517cbdf29cf949f540f05))
* **issues:** recognise full-length API project keys ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **issues:** reconnect after native account restoration ([#10](https://github.com/aiconsultancy/subspace/issues/10)) ([5fb31c6](https://github.com/aiconsultancy/subspace/commit/5fb31c6279b7428cc6ec01e3fe5d26969afae48f))
* **issues:** restart the hidden hub when the account changes ([a2d27df](https://github.com/aiconsultancy/subspace/commit/a2d27df61d9931eb59c400b03e8c0b6dcab39bed))
* **issues:** show a NETT initiative's rows under All teams ([9e17865](https://github.com/aiconsultancy/subspace/commit/9e17865484c98bc8e3641c62d663c19c53b7be84))
* **subspace:** keep a NETT sign-in to its own API and count only real renewals ([eb5b8ee](https://github.com/aiconsultancy/subspace/commit/eb5b8ee84f92da8ff8ff06dff25e96f8147b6c81))

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
