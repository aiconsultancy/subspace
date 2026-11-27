# Changelog

Release Please adds an entry for each release of the Subspace Framework, from
the conventional commits that touch `subspace-framework/`. See
[Publishing the Framework](docs/publishing.md).

## [0.8.0](https://github.com/aiconsultancy/subspace/compare/subspace-framework-v0.7.0...subspace-framework-v0.8.0) (2026-11-27)


### ⚠ BREAKING CHANGES

* **framework:** BoardDescriptor and ValidatedBoard have a new public field former_ids, so code that builds a BoardDescriptor must set it (Vec::new() for none), and ProductError has the new variants FormerIdInUse, DuplicateFormerId, ReservedFormerId and ReservedRenameTarget, so an exhaustive match on it must handle them.

### Features

* **desktop:** show contextual decision actions and refresh controls ([#188](https://github.com/aiconsultancy/subspace/issues/188)) ([090ad8f](https://github.com/aiconsultancy/subspace/commit/090ad8f1b9a2637a40d48d561b75d61092a002f3))
* **framework:** read saved data under a Space's former ids ([1990dcf](https://github.com/aiconsultancy/subspace/commit/1990dcfcf59feb423710bc758811420def555097))

## [0.7.0](https://github.com/aiconsultancy/subspace/compare/subspace-framework-v0.6.0...subspace-framework-v0.7.0) (2026-11-26)


### ⚠ BREAKING CHANGES

* **framework:** ProductBuild has a new public field, pre_startup, and ProductDescriptor and ValidatedProduct have a new public field, command_mount, of the new public type subspace_product::CommandMount. Code that builds any of them as a struct literal must set it; `pre_startup: None` and `command_mount: CommandMount::Prefixed` keep today's behaviour. ProductBuild::new builds one with every optional field unset. The exhaustive public enum ProductError has a new variant, RootMountConflict, so exhaustive matches on it need an arm for it. mcp::served_catalog returns GroupedCatalog instead of Catalog; read the tools with `.served_catalog().tools()`. Products may declare less than before. When the command tree is generated, in Prefixed mode too:
    - a generated root leaf command or alias may not be a word start-up
      claims (about, --about, --version, __product_metadata, mcp, and upgrade
      when the product has an update source) or __complete-word;
    - a generated group may not be mcp, upgrade with an update source, or
      __complete-word;
    - no generated command name or alias, at any depth, may start with `-`;
    - a root-level alias may not equal a group name;
    - no generated flag may share a name or any spelling (long, short, hidden
      alias or `negate` token, clause flags included) with a flag the
      product's tree declares at the root.

    Any of these fails command generation with exit code 1. Product root
    flags no longer reach generated tool input. The default CLI reports an
    unknown root command as `Unknown command: <words>` instead of usage-lib's
    `unexpected word: <word>`.
* **framework:** `Channel::current()` is no longer `const`; the first read fixes the channel for the process. `product_data_name()` takes no argument. Every config, state, cache and log path the Framework manages now uses one directory name, the data name, in the platform config or state directory. The default agents directory, `~/.config/subspace/copilot/agents`, is unchanged. The Keychain service `<data name>.github` uses the data name too, as does any service an app derives from `state::directory_name()`, such as Subspace's `<data name>.fleet-api`. The account session store keeps the namespace the app passes. The data name is the name an app passes to `subspace_framework::product::configure_data_name`. Otherwise it is the binary name, which `prepare_context`, `record_new_install_theme` or `launch` sets, whichever runs first. A pre-release channel adds `-<type>`. For an app whose binary is not `subspace`, such as `harbor`, these are now looked for under `harbor[-<type>]` instead of `subspace[-<type>]`:
    - the shared state directory's caches, the signed-in GitHub account list,
      and the agent sessions and chat workspaces;
    - `config.json` and `copilot.json` as the core reads them;
    - the Keychain service `subspace[-<type>].github`, and
      `subspace[-<type>].fleet-api` for an app that uses the fleet client.

    The Settings state and config files, the saved PATH and the app log were
    already under `harbor[-<type>]` and stay there. No files or Keychain items
    are moved.

    To keep the old shared location, call `configure_data_name("subspace")`
    before `prepare_context`, `record_new_install_theme` and `launch`. It also
    works after an early read. The Settings files, the saved PATH and the log
    then also use `subspace`: one directory name is now the only layout.

    If a data path is read before the data directory is named, the process
    falls back to `subspace`. While that fallback is still implicit and differs
    from the binary name, `prepare_context`, `record_new_install_theme` and
    `launch` stop with an error before reading or writing anything.

### Features

* **desktop:** link decisions and refine workspace views ([#170](https://github.com/aiconsultancy/subspace/issues/170)) ([b44d7b4](https://github.com/aiconsultancy/subspace/commit/b44d7b4686b5dbd47b5f483c9b16f5447b7827ae))
* **framework:** add root command mounts and command composition ([#145](https://github.com/aiconsultancy/subspace/issues/145)) ([3344cbf](https://github.com/aiconsultancy/subspace/commit/3344cbf48b2418368c3e845aa374c64827976a3c))
* **framework:** let an app name its data directory and release channel ([#142](https://github.com/aiconsultancy/subspace/issues/142)) ([ae2d70e](https://github.com/aiconsultancy/subspace/commit/ae2d70e3a21aec259a50e99609c1749549d20096))

## [0.6.0](https://github.com/aiconsultancy/subspace/compare/subspace-framework-v0.5.0...subspace-framework-v0.6.0) (2026-11-26)


### ⚠ BREAKING CHANGES

* **framework:** ProductBuild has a new public field, product_themes. Code that builds ProductBuild as a struct literal must set it; `product_themes: &[]` keeps today's behaviour. record_new_install_theme takes the product's theme documents as a third argument and checks them as start-up does before it writes; pass the same slice, or `&[]`. A custom theme file can no longer name a variant `Default Light` or `Default Dark`, the two themes GPUI Kit preloads: it is skipped with a notice until the variant is renamed in the file and selected again.
* **framework:** an app that offers Copilot chat must enable the subspace-framework `copilot` feature; `agent` and `host` no longer build it.

### Features

* **framework:** build the Copilot chat engine only with a copilot feature ([92d2a86](https://github.com/aiconsultancy/subspace/commit/92d2a8685308d420bc30aca70385798e6ff7210c))
* **framework:** let a product bundle its own theme documents ([17c2d80](https://github.com/aiconsultancy/subspace/commit/17c2d80f82f13a4ce3d1ab90b8d7191e2d2898d5))
* **framework:** let apps register search reference kinds ([#143](https://github.com/aiconsultancy/subspace/issues/143)) ([d806293](https://github.com/aiconsultancy/subspace/commit/d80629389e1f0097333b24948d231a2e1ad730e0))


### Bug Fixes

* **desktop:** follow-up fixes for the optional Copilot build ([0f52440](https://github.com/aiconsultancy/subspace/commit/0f52440c80e2804b2bf3479837550bbe229bf831))
* **framework:** check the exact kind in ReferenceIdentity::of_kind ([01a5b6d](https://github.com/aiconsultancy/subspace/commit/01a5b6d89ceea91eb4d0a864061cbfa894816cca))
* **framework:** explain a refused cloud replay in a toast ([7811fb7](https://github.com/aiconsultancy/subspace/commit/7811fb77c143cfb1345269675eddc70c312d0e97))
* **framework:** keep Save from writing a Copilot preference without Copilot ([af47a90](https://github.com/aiconsultancy/subspace/commit/af47a90d5bfdd6712848e5951db6c2c0dc23c7f0))
* **framework:** keep tool arguments in order in Pi and ACP summaries ([bb8946f](https://github.com/aiconsultancy/subspace/commit/bb8946f877cba126a81607a779cda4e2f707d30c))
* **framework:** stop suggesting Copilot in a build without it ([30ba9c3](https://github.com/aiconsultancy/subspace/commit/30ba9c399ee6d6fac0d2c9bc886e21aea99212d1))

## [0.5.0](https://github.com/aiconsultancy/subspace/compare/subspace-framework-v0.4.0...subspace-framework-v0.5.0) (2026-11-25)


### Features

* **framework:** say which start-up routes read the app's configuration ([e47dd15](https://github.com/aiconsultancy/subspace/commit/e47dd15218d07762a75445c472db9c8fddae2549))


### Bug Fixes

* **desktop:** public release review fixes ([8b63c2f](https://github.com/aiconsultancy/subspace/commit/8b63c2f47dffafa1cada72587b526fb44274b954))
* **framework:** quit when the start-up failure window closes ([d4aab96](https://github.com/aiconsultancy/subspace/commit/d4aab9678b3c57517fcec72867e2b2d3af6aa7b8))

## [0.4.0](https://github.com/aiconsultancy/subspace/compare/subspace-framework-v0.3.0...subspace-framework-v0.4.0) (2026-11-24)


### ⚠ BREAKING CHANGES

* **framework:** UpdateSource has a new public field, `exclusive`.
* **framework:** AccountRegistration has a new public field, `required`.

### Features

* **desktop:** make the public release ready to download and try ([1781a57](https://github.com/aiconsultancy/subspace/commit/1781a5712a575e6e6e36e90cb77a14c07a2fa99b))
* **framework:** ask for a required account's sign-in over the window ([3e138f4](https://github.com/aiconsultancy/subspace/commit/3e138f47ef26ec1cdb2d3187f7b00dd407dfb9f8))
* **framework:** let a board hide records from search without deleting them ([5da63a8](https://github.com/aiconsultancy/subspace/commit/5da63a88bb3466a3f3cfdc682359f76815d31faa))
* **framework:** let a build variant install only its own archives ([3b7dc64](https://github.com/aiconsultancy/subspace/commit/3b7dc6445b9470e17deb89b94b0a61ebb24dfb64))
* **framework:** show a start-up failure in a window ([7a66e38](https://github.com/aiconsultancy/subspace/commit/7a66e383dac75c92a5bafec753068dfab53423a7))
* **irc:** connect Channels from the signed-in account with no settings ([7f6f9c3](https://github.com/aiconsultancy/subspace/commit/7f6f9c3427200d4b2aa25ea09acd61a8646b5120))
* **irc:** tracker commands and a message toolbar in Channels ([2f8eff0](https://github.com/aiconsultancy/subspace/commit/2f8eff0c38bb7b9090ace9b9e004f5a82e2d68a7))


### Bug Fixes

* **framework:** keep a failed sign-in attempt from unlocking the app ([bb59aff](https://github.com/aiconsultancy/subspace/commit/bb59aff8fe3197a5d519be5616dc8460ba97a73b))
* **framework:** keep chunks on doubt and sweep only what a chunk holds ([078f491](https://github.com/aiconsultancy/subspace/commit/078f4918e1693427b37d21b748f2709b543716f5))
* **framework:** keep focus and outside routes out from behind sign-in ([32a2c3c](https://github.com/aiconsultancy/subspace/commit/32a2c3c3e5bb58d429ab39c481bd97fb98d2e018))
* **framework:** keep hidden records out of related items, reads and candidates ([ba49c6c](https://github.com/aiconsultancy/subspace/commit/ba49c6c4767afe635f62513b0190a6be23c52af4))
* **framework:** keep plain secrets verbatim and verify split ones ([d41d146](https://github.com/aiconsultancy/subspace/commit/d41d146858c2da9f908f665a3e320711181bb756))
* **framework:** land the reviewed Windows credential split ([19f8187](https://github.com/aiconsultancy/subspace/commit/19f8187acee6711df165d644c96782c57a0da06e))
* **framework:** refuse every action but Quit behind the sign-in dialog ([5efd132](https://github.com/aiconsultancy/subspace/commit/5efd1328f61d26f6a674e06f646e8672b70132d3))
* **framework:** split long secrets across Windows credentials ([6280fe2](https://github.com/aiconsultancy/subspace/commit/6280fe2c121a1b67eff06ea10e33166224c05c1c))
* **framework:** sweep keychain chunks only at sign-out, and only exactly ([91b404c](https://github.com/aiconsultancy/subspace/commit/91b404c56fda20e47561d0c09fb2ddffe2bc28f4))
* **framework:** treat an identity provider outage as offline ([4deb986](https://github.com/aiconsultancy/subspace/commit/4deb98670e8d93b4fd8261fecce10a5e0b2e50f6))

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
