# Changelog

Release Please adds an entry for each release of the Subspace App, from the
conventional commits that touch `subspace-desktop/`. See
[Releases](docs/ways-of-working.md#releases).

## [0.14.0](https://github.com/aiconsultancy/subspace/compare/subspace-desktop-v0.13.0...subspace-desktop-v0.14.0) (2026-11-28)


### ⚠ BREAKING CHANGES

* **framework:** OverflowUnit::refresh is removed in favour of OverflowUnit::icon_button, and plan_overflow takes the row's width as an Option.

### Features

* **desktop:** align native Space toolbars and inspectors ([#218](https://github.com/aiconsultancy/subspace/issues/218)) ([290fb9b](https://github.com/aiconsultancy/subspace/commit/290fb9bccc874156c3fe27fae3dac411c71e5468))
* **fleet:** enable private Fleet Audit ([#220](https://github.com/aiconsultancy/subspace/issues/220)) ([2adc4fd](https://github.com/aiconsultancy/subspace/commit/2adc4fd8b9d1a79aa98a416884775014cf41fae2))
* **irc:** add Comms and keep replies in threads ([8458afd](https://github.com/aiconsultancy/subspace/commit/8458afd2c09bef513c7ab611b69c4241c3e48244))
* **irc:** compose replies in the thread inspector ([4b72b74](https://github.com/aiconsultancy/subspace/commit/4b72b74f3e0785b35fe2566a9ec15593dd6ee9f1))
* **irc:** compose replies in the thread inspector ([df08b82](https://github.com/aiconsultancy/subspace/commit/df08b82511d09336ce4217f7165eca4631fc2b98))


### Bug Fixes

* **decisions:** page results and keep rows visible while searching ([e9a7c0f](https://github.com/aiconsultancy/subspace/commit/e9a7c0f91214dc58d922d9985a6f3f5c254e1025))
* **decisions:** repair ID search and add cursor paging ([2b59db7](https://github.com/aiconsultancy/subspace/commit/2b59db71874599f92ed7a97ff09de60679739032))
* **desktop:** discard stale search results and message menus ([228b5f7](https://github.com/aiconsultancy/subspace/commit/228b5f7da19a9a7b53eeb2d755951dc8c92c296b))
* **desktop:** restore file drops, stabilise Ship details and wrap issue titles ([3fa6a9b](https://github.com/aiconsultancy/subspace/commit/3fa6a9b38dfc3e4710bdc3d374367b53ffc5cbbb))
* **fleet-console:** preserve selected crew outcomes ([ebd3275](https://github.com/aiconsultancy/subspace/commit/ebd32758f2b6516887edcb6f3eac0b09ec3f7cbc))
* **fleet-console:** preserve selected crew outcomes ([0900cd6](https://github.com/aiconsultancy/subspace/commit/0900cd6baee2d0d96087166ba827eb60cd6ab0f2))
* **fleet-console:** retain latest known crew activity through outages ([0871660](https://github.com/aiconsultancy/subspace/commit/087166009a01edd10c5afb9accad1291845c8129))
* **framework:** settle the follow-ups from the menu bar and hairlines reviews ([#212](https://github.com/aiconsultancy/subspace/issues/212)) ([afbeb85](https://github.com/aiconsultancy/subspace/commit/afbeb85683f47662611172807685b937e51f0f20))
* **irc:** accept mixed file drops and web attachments ([aa84d2a](https://github.com/aiconsultancy/subspace/commit/aa84d2a0be2ca241e2567bf0352835ee1dabe8f4))
* **irc:** clear notifications when removing a channel ([99ef7eb](https://github.com/aiconsultancy/subspace/commit/99ef7eb1912b0a18904dfae4b2c3b24f8c496b3b))
* **irc:** disambiguate formatting imports for rustdoc ([745ee70](https://github.com/aiconsultancy/subspace/commit/745ee708bec0047b8aee849b26ad98b32095806c))
* **irc:** distinguish permalink history from reconnect catch-up ([56cd039](https://github.com/aiconsultancy/subspace/commit/56cd039ff5874da8f334b8e2251b3335bbe04c3c))
* **irc:** filter claimed channels before cloning them ([d19844c](https://github.com/aiconsultancy/subspace/commit/d19844ca5a9582549985f1267bc84549ea6f2d85))
* **irc:** improve fleet threads and message controls ([4075907](https://github.com/aiconsultancy/subspace/commit/4075907c5bc8eed9d26e71acd0ff6c3f2f0d8418))
* **irc:** inset hover actions and remove the thread highlight border ([1ccdca9](https://github.com/aiconsultancy/subspace/commit/1ccdca94a6bdb11c6bfff5aeec33f8087081bbed))
* **irc:** join linked channels before loading threads ([0024ac6](https://github.com/aiconsultancy/subspace/commit/0024ac637c8dafc86cee7544583b3d7aeb41790f))
* **irc:** keep earlier thread drafts reachable ([0d709c1](https://github.com/aiconsultancy/subspace/commit/0d709c1b585738753d5aa183d069f50ef1528636))
* **irc:** keep Ship details stale until the reconnect is ready ([b3d6f1b](https://github.com/aiconsultancy/subspace/commit/b3d6f1b1ffe18ec0f73240dea41df23f71eb6a4e))
* **irc:** keep Ship details steady through refreshes ([7710ec0](https://github.com/aiconsultancy/subspace/commit/7710ec0c169051609824bed758ba55745d0a1b3e))
* **irc:** keep Stop available in the Thread inspector ([df05327](https://github.com/aiconsultancy/subspace/commit/df053278be2672032359976bcc27e6efa1c561f4))
* **irc:** keep the transcript visually grouped ([3ac3418](https://github.com/aiconsultancy/subspace/commit/3ac341873a67b76d4048d19fcf347b9fe54a72b2))
* **irc:** preserve thread access and clear account activity ([36f3079](https://github.com/aiconsultancy/subspace/commit/36f30792e31062a8d531f808bdeaa98fc0307583))
* **irc:** preserve unread thread replies and require channel membership ([7ce0921](https://github.com/aiconsultancy/subspace/commit/7ce092111fd0142b22695f0aec12fca8c5e0ad61))
* **irc:** require a fresh channel join after reconnecting ([1c2883f](https://github.com/aiconsultancy/subspace/commit/1c2883f92797ce0eaa782fe9b1149ba0e2d9f921))
* **irc:** retain crew while a reconnected feed fills in ([871a3a5](https://github.com/aiconsultancy/subspace/commit/871a3a5ba2aad318a9c4cb2f23eb4deba0d37f6f))
* **irc:** update the draft shortcut after sending or recalling ([a074637](https://github.com/aiconsultancy/subspace/commit/a074637e0b924b68e095d3b0277aba263a77621d))
* **issues:** nest sub-issues in the desktop list (SUB-89) ([886da1e](https://github.com/aiconsultancy/subspace/commit/886da1ea0dcca49b8f33323756fca42b1908f49e))
* **issues:** stabilise decision paging and show maker names ([971c298](https://github.com/aiconsultancy/subspace/commit/971c298252ddaab165969d6e90843d5c348ae609))
* **issues:** stabilise decision paging and show maker names ([3302111](https://github.com/aiconsultancy/subspace/commit/3302111d53e362be0a9facea51ba139210377cae))
* **issues:** wrap long titles beside Properties ([7f30f58](https://github.com/aiconsultancy/subspace/commit/7f30f586a30150ed1237551ed77fdbb2055f5c38))
* **search:** reveal issues and decisions hidden by filters ([4b53aa2](https://github.com/aiconsultancy/subspace/commit/4b53aa2d7fededb3c8fcd6a56d2128356b819cfb))
* **subspace:** build the terminal from a shared offline Ghostty cache ([#155](https://github.com/aiconsultancy/subspace/issues/155)) ([efb887b](https://github.com/aiconsultancy/subspace/commit/efb887b3e21350260062e0adbf1e8a7ff4cd518e))

## [0.13.0](https://github.com/aiconsultancy/subspace/compare/subspace-desktop-v0.12.1...subspace-desktop-v0.13.0) (2026-11-27)


### ⚠ BREAKING CHANGES

* apps built on the Framework use GPUI Kit 0.11 and GPUI Fast.
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
* **desktop:** the default build has no Copilot chat. Build with the `copilot` feature to keep it.
* **issues:** the All initiatives choice is removed when the app has an API backend.

### Features

* **api-client:** keep one remembered initiative selected for every member ([bcf4687](https://github.com/aiconsultancy/subspace/commit/bcf468771f47d577359b9f3021ed410b66055db0))
* **decisions:** propose and review decisions in the desktop ([c585c9b](https://github.com/aiconsultancy/subspace/commit/c585c9b0b88252209a4d48907006fc5cf6395b0d))
* **desktop:** add a build-gated AgentsView Space ([#173](https://github.com/aiconsultancy/subspace/issues/173)) ([b8b9de6](https://github.com/aiconsultancy/subspace/commit/b8b9de6b4229fb46eb14104799993fc05908cc11))
* **desktop:** add Projects and milestone management ([f6b5b02](https://github.com/aiconsultancy/subspace/commit/f6b5b026addc05b7a364a4f3e7fba0b743d83261))
* **desktop:** add Projects and milestone management ([554fd86](https://github.com/aiconsultancy/subspace/commit/554fd86878f6177917159da024cb825ae2bf4391))
* **desktop:** ask for sign-in at start-up with an API backend ([48779af](https://github.com/aiconsultancy/subspace/commit/48779af75ee65857c14113ac3c47095f16334f5f))
* **desktop:** build the app without Copilot chat by default ([9ce2837](https://github.com/aiconsultancy/subspace/commit/9ce283734ef3fa1ea17b70762e686dc09b639592))
* **desktop:** build with no API backend ([6e64acd](https://github.com/aiconsultancy/subspace/commit/6e64acd0cb879e8c4f824ab5fa4855cfc38a56b9))
* **desktop:** edit issue descriptions as formatted Markdown ([#200](https://github.com/aiconsultancy/subspace/issues/200)) ([22b716a](https://github.com/aiconsultancy/subspace/commit/22b716a90a068b812a305aff9e2d6b06ab9d7cc3))
* **desktop:** edit projects and drill into milestones ([2615083](https://github.com/aiconsultancy/subspace/commit/2615083a6f0c990cb98fb01d57fdba6834531a29))
* **desktop:** edit projects and drill into milestones ([ded616a](https://github.com/aiconsultancy/subspace/commit/ded616a9c862f2b90822ee1061669aa6b5998cc5))
* **desktop:** link decisions and refine workspace views ([#170](https://github.com/aiconsultancy/subspace/issues/170)) ([b44d7b4](https://github.com/aiconsultancy/subspace/commit/b44d7b4686b5dbd47b5f483c9b16f5447b7827ae))
* **desktop:** make the public release ready to download and try ([1781a57](https://github.com/aiconsultancy/subspace/commit/1781a5712a575e6e6e36e90cb77a14c07a2fa99b))
* **desktop:** match AgentsView with Twilight Two ([#183](https://github.com/aiconsultancy/subspace/issues/183)) ([661c213](https://github.com/aiconsultancy/subspace/commit/661c2137bda24413a50e5d7c8e4737310007557b))
* **desktop:** nest issues under trackers and attach files to issues ([#199](https://github.com/aiconsultancy/subspace/issues/199)) ([33a7dfd](https://github.com/aiconsultancy/subspace/commit/33a7dfde91db5900d05ef5e09a910fcf7005ad26))
* **desktop:** publish the no-API build beside the standard one ([6de09fb](https://github.com/aiconsultancy/subspace/commit/6de09fbb84290aca9faa4ed338f1c100dffa16d2))
* **desktop:** read a connection file at start-up ([6d3f865](https://github.com/aiconsultancy/subspace/commit/6d3f86582d7f411e6de408790a3fd011902c1ad9))
* **desktop:** show activity with the Subspace mark ([#202](https://github.com/aiconsultancy/subspace/issues/202)) ([23186a7](https://github.com/aiconsultancy/subspace/commit/23186a7429c0a219d7ccff1ff9abcbc1dd661773))
* **desktop:** show contextual decision actions and refresh controls ([#188](https://github.com/aiconsultancy/subspace/issues/188)) ([090ad8f](https://github.com/aiconsultancy/subspace/commit/090ad8f1b9a2637a40d48d561b75d61092a002f3))
* **desktop:** start Channels, Issues and Decisions on in release builds ([b1ec1d5](https://github.com/aiconsultancy/subspace/commit/b1ec1d5990de5bd3239e8ebbc4f4a181cd1413b7))
* **framework:** demonstrate the tracker parent picker ([#203](https://github.com/aiconsultancy/subspace/issues/203)) ([a3eb49b](https://github.com/aiconsultancy/subspace/commit/a3eb49b4fe15a84c3e012974914f75a8fe45fb68))
* **framework:** give the menu bar idiomatic menus on each platform ([#160](https://github.com/aiconsultancy/subspace/issues/160)) ([d83200f](https://github.com/aiconsultancy/subspace/commit/d83200f9633b924765df321396e24c0cb8b43ccb))
* **framework:** let an app name its data directory and release channel ([#142](https://github.com/aiconsultancy/subspace/issues/142)) ([ae2d70e](https://github.com/aiconsultancy/subspace/commit/ae2d70e3a21aec259a50e99609c1749549d20096))
* **irc:** close Channels for people in no initiative, hold for the first read and name an unreachable API ([e40af0f](https://github.com/aiconsultancy/subspace/commit/e40af0fee7a565ad354ccc32b34733df1cfb7628))
* **irc:** connect Channels from the signed-in account with no settings ([7f6f9c3](https://github.com/aiconsultancy/subspace/commit/7f6f9c3427200d4b2aa25ea09acd61a8646b5120))
* **irc:** connect to the signed-in account's fleet servers without settings ([e7abf9f](https://github.com/aiconsultancy/subspace/commit/e7abf9f1c3ce5a6d9f1fd3931170b5b6d412a6c3))
* **irc:** discover new direct messages after reconnecting ([a64aff2](https://github.com/aiconsultancy/subspace/commit/a64aff2c7776dff5a404207d293aa5a6cf79f3d1))
* **irc:** fetch a SASL login from a credential source at each connect ([df99582](https://github.com/aiconsultancy/subspace/commit/df995821223ab3f1a2e7137c5fefa91e8d24c708))
* **irc:** keep account networks off without an API backend ([cd73c25](https://github.com/aiconsultancy/subspace/commit/cd73c250ad6f7c1693484bc01db3c0d5da647cf0))
* **irc:** show the account's networks read-only in Settings › Channels ([32ce9ca](https://github.com/aiconsultancy/subspace/commit/32ce9ca70c58960167d28a4d74d76d2998343f6b))
* **irc:** tracker commands and a message toolbar in Channels ([2f8eff0](https://github.com/aiconsultancy/subspace/commit/2f8eff0c38bb7b9090ace9b9e004f5a82e2d68a7))
* **issues:** assign trackers and confirm child updates ([#187](https://github.com/aiconsultancy/subspace/issues/187)) ([1617d83](https://github.com/aiconsultancy/subspace/commit/1617d83b7008db727f5826cdf783225ede7bdab3))
* **issues:** show and edit issue owners ([#209](https://github.com/aiconsultancy/subspace/issues/209)) ([c6b2f60](https://github.com/aiconsultancy/subspace/commit/c6b2f60c55631f1a9091c5ae28151b9fdabb2a56))
* **issues:** show only the selected initiative and drop All initiatives ([943c920](https://github.com/aiconsultancy/subspace/commit/943c9208a89f2289bc5b351f05472cb0bddf6a7f))
* let officers inspect images and share files in IRC ([#139](https://github.com/aiconsultancy/subspace/issues/139)) ([77aa4bc](https://github.com/aiconsultancy/subspace/commit/77aa4bc3d19580386ce29f2284d33c33152500f7))
* **subspace-api-client:** read the username and ask for an IRC credential ([0d947db](https://github.com/aiconsultancy/subspace/commit/0d947dbfde65a46429094f9a7c13c7ddeea4c2aa))
* **web:** sign a web Space in with the app's account on macOS ([8c3f03d](https://github.com/aiconsultancy/subspace/commit/8c3f03db5668e47d0d4647568e5e73022cbd48e4))
* **web:** sign a web Space in with the app's account on macOS ([17243a3](https://github.com/aiconsultancy/subspace/commit/17243a303a3ba2328f50f0d99757da25ffc76710))
* **web:** start a signed-in web Space where the app's state says ([5074b37](https://github.com/aiconsultancy/subspace/commit/5074b3756c78529900ae9cedc50b30b910750378))
* **web:** start a signed-in web Space where the app's state says ([4e9b925](https://github.com/aiconsultancy/subspace/commit/4e9b9256f2d8e7efbd1d5e3549ed0feb16595f4c))


### Bug Fixes

* **desktop:** check connection values after the variables and blame their source ([30ea130](https://github.com/aiconsultancy/subspace/commit/30ea130e1648a55c225e0078a01cb79c4d1343f8))
* **desktop:** check sign-in settings at start-up and open dev sign-in on a hosted API ([f7237dd](https://github.com/aiconsultancy/subspace/commit/f7237dd23f5eac776a2d2104c008392903df39f7))
* **desktop:** check the Copilot build's licences too ([12f43cb](https://github.com/aiconsultancy/subspace/commit/12f43cbec2b1ceb40d3f16869c5f8c79e296936b))
* **desktop:** follow-up fixes for the optional Copilot build ([0f52440](https://github.com/aiconsultancy/subspace/commit/0f52440c80e2804b2bf3479837550bbe229bf831))
* **desktop:** guard milestone edits during scope changes ([2de45bf](https://github.com/aiconsultancy/subspace/commit/2de45bf14f75789801c461a9f2f4e239ddacf9fe))
* **desktop:** guide recovery from connection failures ([e4c41dc](https://github.com/aiconsultancy/subspace/commit/e4c41dc326d430ed65fad319746dfd5280ab114f))
* **desktop:** improve connection recovery and IRC threads ([bcdd4a2](https://github.com/aiconsultancy/subspace/commit/bcdd4a286bab518e0fc47991a5c2bb8ab215faaf))
* **desktop:** keep updates in a build with no API backend ([f0e4503](https://github.com/aiconsultancy/subspace/commit/f0e45034fff31e511f5aca9c647d124f7040de1f))
* **desktop:** let a variable complete or correct the connection file ([136e65a](https://github.com/aiconsultancy/subspace/commit/136e65afc1979c68883c1e283ed171c2593d69ee))
* **desktop:** let About, --version and upgrade run past a broken setting ([580c5e7](https://github.com/aiconsultancy/subspace/commit/580c5e769d43df8765fcf1b376bcd09023539836))
* **desktop:** offer no account when development sign-in cannot reach its API ([d786e8f](https://github.com/aiconsultancy/subspace/commit/d786e8f94b24d098e17f32a56490162783a785bd))
* **desktop:** preserve direct IRC addressing and recovery hints ([34737a4](https://github.com/aiconsultancy/subspace/commit/34737a4290481acc345da72bf0a39c6200b1b4bb))
* **desktop:** preserve project fields and milestone navigation ([66c340d](https://github.com/aiconsultancy/subspace/commit/66c340d280ff39ece3da96cf2ac05ec1baaf2343))
* **desktop:** public release review fixes ([8b63c2f](https://github.com/aiconsultancy/subspace/commit/8b63c2f47dffafa1cada72587b526fb44274b954))
* **desktop:** refuse a bad SUBSPACE_NO_API in the PowerShell installer ([192a2a9](https://github.com/aiconsultancy/subspace/commit/192a2a94f6e5cfc369b34ad7dd7ce5797a1595d7))
* **desktop:** restore milestone scroll after linked issues load ([d24fdc1](https://github.com/aiconsultancy/subspace/commit/d24fdc1e0e0333a807fdba6325726392687837f2))
* **desktop:** show Fleet Console in a build with no API backend ([ad9f98d](https://github.com/aiconsultancy/subspace/commit/ad9f98d60b253c6fa32c5e0fb104e766adabad40))
* **desktop:** stabilise PR references and scope decision reads ([#177](https://github.com/aiconsultancy/subspace/issues/177)) ([17305a3](https://github.com/aiconsultancy/subspace/commit/17305a3e4290270a4281f360f49257a760d70316))
* **desktop:** update a build with no API backend only from its own archives ([931ce40](https://github.com/aiconsultancy/subspace/commit/931ce407a5733ab1a787634426a9eb6edfac5432))
* **fleet:** show the link the service last announced ([#190](https://github.com/aiconsultancy/subspace/issues/190)) ([a93a213](https://github.com/aiconsultancy/subspace/commit/a93a213f8f88ac200c2d4de28c1b1032ad3bf7e8))
* **framework:** keep plain secrets verbatim and verify split ones ([d41d146](https://github.com/aiconsultancy/subspace/commit/d41d146858c2da9f908f665a3e320711181bb756))
* **framework:** land the reviewed Windows credential split ([19f8187](https://github.com/aiconsultancy/subspace/commit/19f8187acee6711df165d644c96782c57a0da06e))
* **framework:** line up the workspace strips and draw one line per edge ([#162](https://github.com/aiconsultancy/subspace/issues/162)) ([c11ffb4](https://github.com/aiconsultancy/subspace/commit/c11ffb4569917840777fc7a9036f012e60bb61cd))
* **framework:** repair [#162](https://github.com/aiconsultancy/subspace/issues/162)'s merge into main ([#206](https://github.com/aiconsultancy/subspace/issues/206)) ([42953c1](https://github.com/aiconsultancy/subspace/commit/42953c162ac6e926d0e15f50c7a98453a5710cff))
* **framework:** restore Markdown editing with GPUI Fast ([#207](https://github.com/aiconsultancy/subspace/issues/207)) ([9a7ad82](https://github.com/aiconsultancy/subspace/commit/9a7ad829031f5831cb5945e9d811bc55086931c3))
* **framework:** sweep keychain chunks only at sign-out, and only exactly ([91b404c](https://github.com/aiconsultancy/subspace/commit/91b404c56fda20e47561d0c09fb2ddffe2bc28f4))
* **github:** count a 400 as a refusal only when GitHub found nothing ([3b689f4](https://github.com/aiconsultancy/subspace/commit/3b689f49cf9714d207525c5d8f734470490520ea))
* **github:** judge each request of a reference read by its own tick ([7b72e73](https://github.com/aiconsultancy/subspace/commit/7b72e734d38043ea334a21f93af9df362ea4e56b))
* **github:** keep an in-flight access decision when another account signs in or out ([9d7bb59](https://github.com/aiconsultancy/subspace/commit/9d7bb59dd2db9eef7ac36b1d310077d1b9f5df0c))
* **irc:** bind each IRC credential to the server it is for ([de96352](https://github.com/aiconsultancy/subspace/commit/de963529e890eca280e7bb731ed2659b16e9c836))
* **irc:** check a staged switch against the account signed in at Save ([07c6e8c](https://github.com/aiconsultancy/subspace/commit/07c6e8c57ee24091838df8ff1979403aa7b4cf64))
* **irc:** close private reactions again when a later initiatives read fails ([5371343](https://github.com/aiconsultancy/subspace/commit/5371343045276fb6a00bc5db033838719c892e43))
* **irc:** close the findings left open on the Channels tracker commands ([a9b4a4b](https://github.com/aiconsultancy/subspace/commit/a9b4a4b8811f4cc020e52f75a9c5ce8b0cb87ef8))
* **irc:** compare a tracker's API base as a URL, its path exactly ([29028f7](https://github.com/aiconsultancy/subspace/commit/29028f7735e962f1f0bf098bac7daeb36b465e59))
* **irc:** compare an unconfirmed command in one canonical form ([cfd92c0](https://github.com/aiconsultancy/subspace/commit/cfd92c02ee302222d1f75f09a60892c6e3628247))
* **irc:** count a decision's length as the API does, with its own message ([5c657d8](https://github.com/aiconsultancy/subspace/commit/5c657d8f2d8d319ccc376e182dd0f841cc599f2b))
* **irc:** count a login's lifetime from its request, at most ten minutes ([f3a740d](https://github.com/aiconsultancy/subspace/commit/f3a740d27579c3e8a1aeb6e01f0aec9cd406cb69))
* **irc:** de-duplicate a rename only within the reports of one NICK ([b9c5156](https://github.com/aiconsultancy/subspace/commit/b9c5156d02d559cb997a569b106a34c437df78a4))
* **irc:** decide a stored line's account from its network alone ([f59b6df](https://github.com/aiconsultancy/subspace/commit/f59b6df909d09fc7443b8e12060ff57232497c84))
* **irc:** drop a conversation's reaction revision once the book holds nothing for it ([559701f](https://github.com/aiconsultancy/subspace/commit/559701f4c4324c45129eb7320174125504ed52bc))
* **irc:** drop a refused sign-in at once and check it again when sent ([ca5690a](https://github.com/aiconsultancy/subspace/commit/ca5690afd2ded9dfa0531a2c8adb98c477b77e2a))
* **irc:** fence an account's networks the moment the account changes ([6e90e04](https://github.com/aiconsultancy/subspace/commit/6e90e044ea5a16e92d432fa5c2b816b8481447d3))
* **irc:** fit tracker summaries and titles to the API's UTF-16 limits ([d14239d](https://github.com/aiconsultancy/subspace/commit/d14239dfce8fa5d04df0c21ea16fcf359cbb20a6))
* **irc:** give /assign no default captain the API records no user for ([cf124fa](https://github.com/aiconsultancy/subspace/commit/cf124fabb095dfde1b11509cb94580bbb4371ecb))
* **irc:** give account networks ids no saved network can take ([f76e2a4](https://github.com/aiconsultancy/subspace/commit/f76e2a45cc1ee7c6fde5d8d715a67ebd8d26e02b))
* **irc:** group and name every fleet channel once the fleet details load ([7473506](https://github.com/aiconsultancy/subspace/commit/7473506685601c1bbdf63201bf9a549efd99b675))
* **irc:** group and name every fleet channel once the fleet details load ([4557fd1](https://github.com/aiconsultancy/subspace/commit/4557fd1a46200a84d55613bb89cdd543d79f90f5))
* **irc:** hold a hand-typed private TAGMSG to the reactions gate ([affdccb](https://github.com/aiconsultancy/subspace/commit/affdccb71c0a1dd5a7212db143546be9e22e56b8))
* **irc:** hold a hand-typed reaction to the reactions gate, stop included ([eb1faf8](https://github.com/aiconsultancy/subspace/commit/eb1faf8c7cb369a843dd4ce33b487805bb959a6b))
* **irc:** keep a message's IRC provenance out of the tracker's Markdown ([6a306b7](https://github.com/aiconsultancy/subspace/commit/6a306b7a205e2682adf316f2aed6358d51abfdc6))
* **irc:** keep a tracker command's draft and say why while another write runs ([9d80c36](https://github.com/aiconsultancy/subspace/commit/9d80c364a7874cb7855bf2c5b627fbb8b24d835d))
* **irc:** keep account history across accounts and keep saved networks working on upgrade ([d1ce36f](https://github.com/aiconsultancy/subspace/commit/d1ce36fa39c25b2ac167fc11fece98564ed69e97))
* **irc:** keep an issued password out of the configuration and require its sign-in ([cc78200](https://github.com/aiconsultancy/subspace/commit/cc78200ff80652d3480da7920daaee4a45331f17))
* **irc:** keep attachment references in the same message ([#123](https://github.com/aiconsultancy/subspace/issues/123)) ([bb17474](https://github.com/aiconsultancy/subspace/commit/bb17474bc68995324f514ebbdebc820cdf30ce03))
* **irc:** keep comma addressing and ignore mentions inside URLs ([414b63b](https://github.com/aiconsultancy/subspace/commit/414b63bcd16274c3c7ffdfbe161fd0a3ae96587f))
* **irc:** keep private reactions closed until the initiatives were read for this account ([77d97f3](https://github.com/aiconsultancy/subspace/commit/77d97f3c33e310292b5f4c6fa5083ed1f945cd0d))
* **irc:** keep request status and reply counts in sync ([70c9054](https://github.com/aiconsultancy/subspace/commit/70c90545959f80907eba97e7d9b8d74c250e7dbf))
* **irc:** keep the Settings switch to an account network honest ([023fdc8](https://github.com/aiconsultancy/subspace/commit/023fdc833836306ff19f87d527a0c9a013795bc1))
* **irc:** never send an unconfirmed create or decision again from the same place ([2a59f6c](https://github.com/aiconsultancy/subspace/commit/2a59f6cc28fd355ecdd8283d4a2d89084a1bedc6))
* **irc:** open the reaction menu at the message it is for ([6aede44](https://github.com/aiconsultancy/subspace/commit/6aede44cf285479017e35dbfa55aeb15fd8a0277))
* **irc:** plan account networks only from the account's own memberships ([6ab56b7](https://github.com/aiconsultancy/subspace/commit/6ab56b73e7cbb38b42b9d9d4671ceef3daf1efdb))
* **irc:** re-export the dialog's scope refusal for tests only ([65a026c](https://github.com/aiconsultancy/subspace/commit/65a026c2f2f5112c9868759a5be8388d93432f72))
* **irc:** redraw Channels when the start-up wait for initiatives times out ([740f27f](https://github.com/aiconsultancy/subspace/commit/740f27fc2e6f8d076fc200b4c31d273564ac8798))
* **irc:** redraw Channels when the start-up wait for initiatives times out ([d7a920d](https://github.com/aiconsultancy/subspace/commit/d7a920d35e057925e89251b4a888109de8a68690))
* **irc:** report an open tracker dialog as the Space's dialog ([fa3d623](https://github.com/aiconsultancy/subspace/commit/fa3d62363ff6a24ae18de6decef1b9423b3ebc1c))
* **irc:** run a completion's run row only for the plan it showed ([12d2e7c](https://github.com/aiconsultancy/subspace/commit/12d2e7cb099e2db24aa3068be8ce98792502dcff))
* **irc:** run a run row only with the client it was shown under ([1e2becd](https://github.com/aiconsultancy/subspace/commit/1e2becd1b860e20e6a75810ad02ad33adb7587fe))
* **irc:** run a tracker command only on Return or a click, never on Tab ([5ba4d4d](https://github.com/aiconsultancy/subspace/commit/5ba4d4db63768e55cc2d8324a919cdb0f5cc57e1))
* **irc:** send a decision's evidence reference as plain text ([1ad73d9](https://github.com/aiconsultancy/subspace/commit/1ad73d9a2a3618b5470aa537e258aaca840746f2))
* **irc:** send a tracker write only to the tracker the Issues scope names ([e4688b4](https://github.com/aiconsultancy/subspace/commit/e4688b43873f4b40f9f2f0a463d4c80ba80b63a9))
* **irc:** show every word a message dialog writes, and fence a decision's body ([d601176](https://github.com/aiconsultancy/subspace/commit/d601176a302dfbbb03359335517117c2189db09d))
* **irc:** start an account network only once its history is restored ([1d041bf](https://github.com/aiconsultancy/subspace/commit/1d041bf07741a89a80118f0818b42affa1b76110))
* **irc:** time a login from the API's clock and keep it through a failed renewal ([433d169](https://github.com/aiconsultancy/subspace/commit/433d169bfcb74e8b341da4804ed57289999fbdeb))
* **irc:** use status badges on hover previews ([75c3a0e](https://github.com/aiconsultancy/subspace/commit/75c3a0e0dd40f4fcec9ad5763927cf383d13d22d))
* **irc:** use status badges on hover previews ([387513f](https://github.com/aiconsultancy/subspace/commit/387513ffaddb98d6da9d6ada75a50c2d2042d77e))
* **irc:** use the paperclip to choose files ([8b15876](https://github.com/aiconsultancy/subspace/commit/8b15876191e481f80fc631cef2291a3e6a5549db))
* **irc:** use the paperclip to choose files ([5b7c6c8](https://github.com/aiconsultancy/subspace/commit/5b7c6c85f0ada3c1c8c7efee003bfc3767de6443))
* **irc:** write from a message dialog only under the account and tracker it opened under ([4c8f66e](https://github.com/aiconsultancy/subspace/commit/4c8f66e4d7315203f1d83a0d86139ffd6ad5c5ff))
* **issues:** say sign-in is unavailable everywhere Issues asks for it ([77afa26](https://github.com/aiconsultancy/subspace/commit/77afa26917a43beed43121181eb850c9a86ae23b))
* **subspace-api-client:** drop a cached issue card when its refresh is refused ([7a188f8](https://github.com/aiconsultancy/subspace/commit/7a188f8bb80dee8ac5046c3dce79e510baf32c9e))
* **web:** address the review findings for web Space sessions ([24c954a](https://github.com/aiconsultancy/subspace/commit/24c954ad399b2f652cc3b08606f969b5aa4c0042))
* **web:** address the review findings for web Space sessions ([13354e5](https://github.com/aiconsultancy/subspace/commit/13354e596d5514dab310c94d06b17794e1c1fca3))


### Performance Improvements

* **irc:** gather tracker facts for completion only in a slash command ([da98982](https://github.com/aiconsultancy/subspace/commit/da98982034a922fd9e592f14388f04946da3b015))
* stop rendering hidden Spaces and speed up Actions rows ([#157](https://github.com/aiconsultancy/subspace/issues/157)) ([98c32a6](https://github.com/aiconsultancy/subspace/commit/98c32a664a4725fdf8bd348a1e241c3acb4ab623))
* **subspace-api-client:** skip a superseded reference read before its request ([26a7678](https://github.com/aiconsultancy/subspace/commit/26a767865247d7b8b87c166e252352e09b5668e6))


### Code Refactoring

* build on GPUI Kit 0.11 with GPUI Fast ([#191](https://github.com/aiconsultancy/subspace/issues/191)) ([90611c3](https://github.com/aiconsultancy/subspace/commit/90611c344337eb1499feae9070f35d83b70be90b))

## [0.12.1](https://github.com/aiconsultancy/subspace/compare/subspace-desktop-v0.12.0...subspace-desktop-v0.12.1) (2026-11-22)


### Bug Fixes

* **desktop:** harden the built-in Fleet Console's start-up ([4bbba18](https://github.com/aiconsultancy/subspace/commit/4bbba1801aa078e92f22c51b623760bca1f9131d))
* **desktop:** harden the built-in Fleet Console's start-up ([addab69](https://github.com/aiconsultancy/subspace/commit/addab69b94456716b053a41e3c0a19e6be2703cd))

## [0.12.0](https://github.com/aiconsultancy/subspace/compare/subspace-desktop-v0.11.0...subspace-desktop-v0.12.0) (2026-11-22)


### Features

* **desktop:** build Fleet Console in, turn Actions on, and add a live fleet fixture ([71bc10a](https://github.com/aiconsultancy/subspace/commit/71bc10ac07fed1798c13569c644b5f0b21399f95))

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
