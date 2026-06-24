# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.17.0](https://github.com/exoma-ch/hyrr/compare/v0.16.3...v0.17.0) (2026-06-24)


### Features

* **stopping:** ³He uses per-isotope catima_He3, not ASTAR×4/3 ([#194](https://github.com/exoma-ch/hyrr/issues/194)) ([#482](https://github.com/exoma-ch/hyrr/issues/482)) ([74b2618](https://github.com/exoma-ch/hyrr/commit/74b2618c6e071bc66c3da5754cb5d1da11b8fb4c))


### Bug Fixes

* **ci:** build WASM before the e2e-tauri frontend build (actually fixes [#457](https://github.com/exoma-ch/hyrr/issues/457)) ([#480](https://github.com/exoma-ch/hyrr/issues/480)) ([3e149d8](https://github.com/exoma-ch/hyrr/commit/3e149d8cf6d79e5c9c7ef9ca985fd22c27dc72df))
* **frontend:** show emission Σ total once in unified hover, not under each isotope ([#462](https://github.com/exoma-ch/hyrr/issues/462)) ([#463](https://github.com/exoma-ch/hyrr/issues/463)) ([90962b6](https://github.com/exoma-ch/hyrr/commit/90962b6c758dabf66ffa2fac75d8710a44da55f0))

## [0.16.3](https://github.com/exoma-ch/hyrr/compare/v0.16.2...v0.16.3) (2026-06-23)


### Bug Fixes

* **mcp:** tracing logs to stderr, not stdout — unblocks hyrr-mcp publish ([#478](https://github.com/exoma-ch/hyrr/issues/478)) ([173e986](https://github.com/exoma-ch/hyrr/commit/173e986d428862392f994d3c7cafbd17ce054191))


### CI

* **release:** use hyrr-release-bot App token, not GITHUB_TOKEN ([#477](https://github.com/exoma-ch/hyrr/issues/477)) ([69cbdbb](https://github.com/exoma-ch/hyrr/commit/69cbdbbe2846e244386674e229747af4f8ee727e))

## [0.16.2](https://github.com/exoma-ch/hyrr/compare/v0.16.1...v0.16.2) (2026-06-23)


### Bug Fixes

* **ci:** pin wasm-pack version for tauri build (macOS) ([#457](https://github.com/exoma-ch/hyrr/issues/457)) ([#475](https://github.com/exoma-ch/hyrr/issues/475)) ([2433f5e](https://github.com/exoma-ch/hyrr/commit/2433f5e18fa184b331bc5a1b550cbc2a8cfe351c))

## [0.16.1](https://github.com/exoma-ch/hyrr/compare/v0.16.0...v0.16.1) (2026-06-23)


### Bug Fixes

* **ci:** build WASM before the tauri desktop build ([#457](https://github.com/exoma-ch/hyrr/issues/457)) ([#474](https://github.com/exoma-ch/hyrr/issues/474)) ([7169a20](https://github.com/exoma-ch/hyrr/commit/7169a202666be8573e14feb727239cceea87788d))
* **e2e:** repair the 3 long-red tests dragging the whole matrix to failure ([#472](https://github.com/exoma-ch/hyrr/issues/472)) ([46f917c](https://github.com/exoma-ch/hyrr/commit/46f917cc868fcbc054b6abe6362975ca69ac5221))

## [0.16.0](https://github.com/exoma-ch/hyrr/compare/v0.15.1...v0.16.0) (2026-06-23)


### Features

* **frontend:** sticky Filters bar between Depth Profile and Production vs Depth ([#469](https://github.com/exoma-ch/hyrr/issues/469)) ([5362ba9](https://github.com/exoma-ch/hyrr/commit/5362ba98457ee8ec0246d88ce8780ccd5d6e78cc))


### Bug Fixes

* **frontend:** production-vs-depth shows per-layer radionuclides (not stable matrix) ([#467](https://github.com/exoma-ch/hyrr/issues/467)) ([0a96496](https://github.com/exoma-ch/hyrr/commit/0a96496b608c03ea8334f91f6d24be62b25e925c))
* **frontend:** show the metastable 'm' in cross-section channel labels ([#468](https://github.com/exoma-ch/hyrr/issues/468)) ([2865edc](https://github.com/exoma-ch/hyrr/commit/2865edc1076681899c8f09842c91ed92d4d16611)), closes [#436](https://github.com/exoma-ch/hyrr/issues/436)


### Miscellaneous

* **deps:** bump nucl-parquet — federated catima shards (our [#252](https://github.com/exoma-ch/hyrr/issues/252) implemented) ([#470](https://github.com/exoma-ch/hyrr/issues/470)) ([a02a03e](https://github.com/exoma-ch/hyrr/commit/a02a03e6bfab73ee8c746bcf44c96d4b51b2eb01))

## [0.15.1](https://github.com/exoma-ch/hyrr/compare/v0.15.0...v0.15.1) (2026-06-23)


### Bug Fixes

* **frontend:** make EOB≠EOC legible — F-18 cooling, cooling=0 cue, e2e guard ([#464](https://github.com/exoma-ch/hyrr/issues/464)) ([3f65bb6](https://github.com/exoma-ch/hyrr/commit/3f65bb6574ce9a41d40810372980076f36492171)), closes [#462](https://github.com/exoma-ch/hyrr/issues/462)


### Miscellaneous

* **deps:** bump nucl-parquet to v0.14.0 (catima isotope fix [#247](https://github.com/exoma-ch/hyrr/issues/247)) ([#465](https://github.com/exoma-ch/hyrr/issues/465)) ([2539158](https://github.com/exoma-ch/hyrr/commit/2539158b073583085d6112bbb32c1bfec10a5ca8))

## [0.15.0](https://github.com/exoma-ch/hyrr/compare/v0.14.0...v0.15.0) (2026-06-22)


### Features

* **mcp:** structured data export — dataset/inventory/emission-curve tools + result cache ([#433](https://github.com/exoma-ch/hyrr/issues/433)) ([f3e749f](https://github.com/exoma-ch/hyrr/commit/f3e749fd10a28675de38fae99516cf50a392a528)), closes [#427](https://github.com/exoma-ch/hyrr/issues/427)
* **share:** transport full custom-material definitions in links + recipient can save ([#96](https://github.com/exoma-ch/hyrr/issues/96)) ([#452](https://github.com/exoma-ch/hyrr/issues/452)) ([0deb391](https://github.com/exoma-ch/hyrr/commit/0deb391de063df4982fbc923df4447560e371e50))
* **tracing:** structured trace spine + opt-in bug-report attach ([#159](https://github.com/exoma-ch/hyrr/issues/159)) ([#456](https://github.com/exoma-ch/hyrr/issues/456)) ([1331a5b](https://github.com/exoma-ch/hyrr/commit/1331a5bed4ebf5b326f4f47484e55799b343a467))


### Bug Fixes

* batch of simulation + UX bug fixes ([#344](https://github.com/exoma-ch/hyrr/issues/344) [#436](https://github.com/exoma-ch/hyrr/issues/436) [#438](https://github.com/exoma-ch/hyrr/issues/438) [#437](https://github.com/exoma-ch/hyrr/issues/437) [#435](https://github.com/exoma-ch/hyrr/issues/435) [#331](https://github.com/exoma-ch/hyrr/issues/331)) ([#454](https://github.com/exoma-ch/hyrr/issues/454)) ([6c56cd5](https://github.com/exoma-ch/hyrr/commit/6c56cd5d4da603cba138a4aa0b0336d341114ec6))
* **core:** build Z↔symbol map from IUPAC table, not abundances ([#450](https://github.com/exoma-ch/hyrr/issues/450)) ([9a38540](https://github.com/exoma-ch/hyrr/commit/9a385400ec33cd29a84da45b436baea919ee52fe)), closes [#444](https://github.com/exoma-ch/hyrr/issues/444)
* **core:** replace user-reachable panics with structured errors ([#355](https://github.com/exoma-ch/hyrr/issues/355)) ([#451](https://github.com/exoma-ch/hyrr/issues/451)) ([35fc4fc](https://github.com/exoma-ch/hyrr/commit/35fc4fc2500bf5e3a016182368f40212e7587ecd))
* **native:** unwrap compute_stack Result so the Python API gets layers ([#448](https://github.com/exoma-ch/hyrr/issues/448)) ([4443a74](https://github.com/exoma-ch/hyrr/commit/4443a7436d864600a4f14bd76fddc90c8e013489)), closes [#444](https://github.com/exoma-ch/hyrr/issues/444)


### Miscellaneous

* **dev:** reproducible hyrr._native build + runnable integration suite ([#443](https://github.com/exoma-ch/hyrr/issues/443)) ([ea04df8](https://github.com/exoma-ch/hyrr/commit/ea04df80fb6404ed11f43d9eb791cd92617c536d))
* sync root package-lock to frontend 0.13.0 / MIT / hyparquet-writer 0.15.1 ([#442](https://github.com/exoma-ch/hyrr/issues/442)) ([4d5fc63](https://github.com/exoma-ch/hyrr/commit/4d5fc636837c2aea1930b7a605a92bd552e98761))


### Documentation

* **claude:** correct stale 'pure Python' architecture → Rust core + bindings ([#449](https://github.com/exoma-ch/hyrr/issues/449)) ([6e1cd38](https://github.com/exoma-ch/hyrr/commit/6e1cd3890c4f8c8579e4d7ca4b155cd3449c8e0a))


### CI

* **release-please:** auto-sync uv.lock + wasm/Cargo.lock on release PRs ([#447](https://github.com/exoma-ch/hyrr/issues/447)) ([094cfd1](https://github.com/exoma-ch/hyrr/commit/094cfd1f6263a1e83fd5061d76dcecb213ab5686))

## [0.14.0](https://github.com/exoma-ch/hyrr/compare/v0.13.0...v0.14.0) (2026-06-17)


### Features

* 3-way current profile constraint system + NshellEC fix ([#406](https://github.com/exoma-ch/hyrr/issues/406)) ([c290c21](https://github.com/exoma-ch/hyrr/commit/c290c21fa88a760412ddf62aa80221025876fd49))
* **compute:** add analytical trapezoid charge solver with triangle-case branching ([0b59699](https://github.com/exoma-ch/hyrr/commit/0b59699441766c45cf6bfa38d98c5074a6efa470)), closes [#404](https://github.com/exoma-ch/hyrr/issues/404)
* CurrentProfilePopup — upload + generate profiles ([#403](https://github.com/exoma-ch/hyrr/issues/403)) ([5e0683a](https://github.com/exoma-ch/hyrr/commit/5e0683a5ed82e506fd4a3e3f18a543235dd4c6fb)), closes [#395](https://github.com/exoma-ch/hyrr/issues/395)
* **mcp:** layer selection on production curves + list_producing_layers ([#431](https://github.com/exoma-ch/hyrr/issues/431)) ([2182fd6](https://github.com/exoma-ch/hyrr/commit/2182fd6d0c1d730e605cc72b2bd41387f2379d70)), closes [#428](https://github.com/exoma-ch/hyrr/issues/428)
* profile trim handles + editable point table — closes [#328](https://github.com/exoma-ch/hyrr/issues/328) ([#408](https://github.com/exoma-ch/hyrr/issues/408)) ([d72cd17](https://github.com/exoma-ch/hyrr/commit/d72cd171afea7ee10dfc508b2acc059c77ca6c78))


### Bug Fixes

* **ci:** ruff import order + remove duplicate workflow_dispatch ([5ddf137](https://github.com/exoma-ch/hyrr/commit/5ddf137542727609ee749f4ea95f6b49ba545b28))
* **frontend:** 3-way constraint solver + auto-fill time inputs in Generate tab ([#407](https://github.com/exoma-ch/hyrr/issues/407)) ([b51ba8d](https://github.com/exoma-ch/hyrr/commit/b51ba8d62f4ded6ce87277ea9385727ee9ae89cf))
* **frontend:** all Generate tab inputs commit on blur/Enter only ([36a21f6](https://github.com/exoma-ch/hyrr/commit/36a21f647e2f61ff952302e05bba39370280dd80))
* **frontend:** beam bar layout — toggle spans current+irradiation, smart time inputs ([4087613](https://github.com/exoma-ch/hyrr/commit/4087613c349118ba0b3811b51c76205d802b102e))
* **frontend:** Generate tab — clickable rows replace radio dots, no spinners ([d50e340](https://github.com/exoma-ch/hyrr/commit/d50e3406236391b2a18b3b2e0a7947cc978ae3ca))
* **frontend:** Generate tab layout — ramps left, values+derive right, no jitter ([61e3afe](https://github.com/exoma-ch/hyrr/commit/61e3afe88610394ed9d1ef06d6d6d38fa77d2af0))
* **frontend:** layer table shows user-defined material name, not parsed formula ([6a99548](https://github.com/exoma-ch/hyrr/commit/6a995486a5ca6f1cd96477d196ca956f56710e50)), closes [#376](https://github.com/exoma-ch/hyrr/issues/376)
* **frontend:** merge constant/profile current into single toggle field ([#399](https://github.com/exoma-ch/hyrr/issues/399)) ([7097df2](https://github.com/exoma-ch/hyrr/commit/7097df2473923bc160ddb2e6f214344c0362c173))
* **frontend:** smart time inputs auto-fill canonical form on Enter/blur ([7be1e01](https://github.com/exoma-ch/hyrr/commit/7be1e01e7690e7c487c7f307bafb298df649d3be))
* **frontend:** switching derived field captures current values first ([a7647ec](https://github.com/exoma-ch/hyrr/commit/a7647ec019225d736f9085a10c32ea5168968d4d))
* **frontend:** thickness input auto-fills canonical form on Enter/blur ([e25d6ae](https://github.com/exoma-ch/hyrr/commit/e25d6aed53fab605b83b5fa5af5ad933b8e63f12))
* **frontend:** vertical derive radio strip + label alignment in Generate tab ([5fd2d02](https://github.com/exoma-ch/hyrr/commit/5fd2d02daa3032817d3d415acb4a27243181ff3a))


### Tests

* **e2e:** fix stale "Define & save" material selector ([#429](https://github.com/exoma-ch/hyrr/issues/429)) ([aad0073](https://github.com/exoma-ch/hyrr/commit/aad0073e7daeb32d76749c235d6de07008bfcd9b)), closes [#426](https://github.com/exoma-ch/hyrr/issues/426)


### Miscellaneous

* add scripts/dev.sh + just recipes for local development ([#400](https://github.com/exoma-ch/hyrr/issues/400)) ([e90586a](https://github.com/exoma-ch/hyrr/commit/e90586a3f187f941ceef9efb797c35819a143e5f))
* **deps:** bump nucl-parquet submodule -&gt; data-2026.6.0 (EAF-2010 removed) ([#430](https://github.com/exoma-ch/hyrr/issues/430)) ([c2bddac](https://github.com/exoma-ch/hyrr/commit/c2bddac5b136a8d51245898515d20bc32e9197c2))
* dev env doesn't pull vtk (local Python on 3.13/NixOS) ([#409](https://github.com/exoma-ch/hyrr/issues/409)) ([41ae12b](https://github.com/exoma-ch/hyrr/commit/41ae12b75c40f289700eee9606e7ab28184853e2))
* ship sample current profile CSV for manual testing ([#397](https://github.com/exoma-ch/hyrr/issues/397)) ([f916938](https://github.com/exoma-ch/hyrr/commit/f9169385c97bb7b84568d7cbcce0a78b1bc0a9dc))
* stop committing WASM binary to git ([#402](https://github.com/exoma-ch/hyrr/issues/402)) ([68c4b31](https://github.com/exoma-ch/hyrr/commit/68c4b31f8aced34f2f4e1ec9476f4958f6f0efdd))
* switch pre-commit → prek; untrack e2e .last-run.json ([#411](https://github.com/exoma-ch/hyrr/issues/411)) ([2142919](https://github.com/exoma-ch/hyrr/commit/2142919f265238e981ede19477f2edfe1f8a8e86))


### Documentation

* **legal:** reconcile license to MIT + add disclaimer, privacy, terms, citation, compliance ([#432](https://github.com/exoma-ch/hyrr/issues/432)) ([645c661](https://github.com/exoma-ch/hyrr/commit/645c661919946b5752eb4ed540acc971dd937e42))


### CI

* unify lint gate on `prek run --all-files` (one SSoT) ([#413](https://github.com/exoma-ch/hyrr/issues/413)) ([4e0be82](https://github.com/exoma-ch/hyrr/commit/4e0be82e6a05713c42fae8eacfe8564cf37f690a))

## [0.13.0](https://github.com/exoma-ch/hyrr/compare/v0.12.1...v0.13.0) (2026-05-28)


### Features

* density prompt for unknown compounds in material popup ([#361](https://github.com/exoma-ch/hyrr/issues/361)) ([#375](https://github.com/exoma-ch/hyrr/issues/375)) ([4d23d88](https://github.com/exoma-ch/hyrr/commit/4d23d8817823ef18fb8b0a8fac948284cfdb48cc))
* **frontend:** density input on layer card ([#361](https://github.com/exoma-ch/hyrr/issues/361)) ([#367](https://github.com/exoma-ch/hyrr/issues/367)) ([f736b95](https://github.com/exoma-ch/hyrr/commit/f736b95d1285b26f8bbc54f4b46ec2f132c27941))
* **frontend:** layer table below layer cards + clickable layer filter ([#379](https://github.com/exoma-ch/hyrr/issues/379)) ([e4cabc3](https://github.com/exoma-ch/hyrr/commit/e4cabc398de9e2d2355bc009ab82702e97095fa9))


### Bug Fixes

* density_g_cm3 flows from material popup to layer config ([#365](https://github.com/exoma-ch/hyrr/issues/365)) ([815d246](https://github.com/exoma-ch/hyrr/commit/815d2464e0e607e515fc429bf9d0bcf5990bf870))
* density_override param on resolve_material — SSoT for all paths ([#368](https://github.com/exoma-ch/hyrr/issues/368)) ([a755fb1](https://github.com/exoma-ch/hyrr/commit/a755fb185efc3534f0dad42c08f2c1106dfccf35))
* emission tabs grey + hover info ([#384](https://github.com/exoma-ch/hyrr/issues/384)) ([9ee5756](https://github.com/exoma-ch/hyrr/commit/9ee575602035f0c11e349a04b9ed3d7bfdd12207))
* **frontend:** emission plot legend always visible + sum in hover ([#378](https://github.com/exoma-ch/hyrr/issues/378)) ([5f9675d](https://github.com/exoma-ch/hyrr/commit/5f9675da29eb87bac1ec9e0c5f70063a7225b1fc))
* **frontend:** harmonize section headers — shared SectionHeader component ([#383](https://github.com/exoma-ch/hyrr/issues/383)) ([#393](https://github.com/exoma-ch/hyrr/issues/393)) ([0bd95e1](https://github.com/exoma-ch/hyrr/commit/0bd95e1bc70a679803cb96f1f62a286c4b6e01d3))
* invalidateExpansion on ALL config mutators — stale state root cause ([#372](https://github.com/exoma-ch/hyrr/issues/372)) ([574fc96](https://github.com/exoma-ch/hyrr/commit/574fc96d304b1b25148296a97c25deb5dbe0db79))
* Layers in section nav + tab styling ([#377](https://github.com/exoma-ch/hyrr/issues/377)) ([2644e1a](https://github.com/exoma-ch/hyrr/commit/2644e1a26406099910ea9fdb9dcbf018b610130b))
* SSoT — depth preview uses same config expansion as simulation ([#366](https://github.com/exoma-ch/hyrr/issues/366)) ([b7f2357](https://github.com/exoma-ch/hyrr/commit/b7f23573c303002589d9117d6d364f8860e57ac1))
* SSoT config loading — fillDensities + single load path ([#374](https://github.com/exoma-ch/hyrr/issues/374)) ([793c493](https://github.com/exoma-ch/hyrr/commit/793c49394ff958428be66924601cfa5e8be4b041))
* β spectrum starts at (0,0) + sum envelope shows in hover ([#380](https://github.com/exoma-ch/hyrr/issues/380) [#381](https://github.com/exoma-ch/hyrr/issues/381)) ([#382](https://github.com/exoma-ch/hyrr/issues/382)) ([0658b69](https://github.com/exoma-ch/hyrr/commit/0658b692931c43f8fe0d689cea23e450774e4aa0))


### Tests

* add Playwright e2e test for current profile simulation ([#328](https://github.com/exoma-ch/hyrr/issues/328)) ([#394](https://github.com/exoma-ch/hyrr/issues/394)) ([012b174](https://github.com/exoma-ch/hyrr/commit/012b174717d39d195f59156caa24f13ccaa10ee2))


### Miscellaneous

* add manual staging deploy script for when runners are stuck ([#364](https://github.com/exoma-ch/hyrr/issues/364)) ([32c91e2](https://github.com/exoma-ch/hyrr/commit/32c91e2c32bcb1cb1e600a0dda665e9bfd39806c))


### CI

* kick deploy ([#362](https://github.com/exoma-ch/hyrr/issues/362)) ([7669aaf](https://github.com/exoma-ch/hyrr/commit/7669aafa07f238c4680387d985f03a726b0c2829))

## [0.12.1](https://github.com/exoma-ch/hyrr/compare/v0.12.0...v0.12.1) (2026-05-28)


### Bug Fixes

* propagate inline density from share URL to Rust backend ([#359](https://github.com/exoma-ch/hyrr/issues/359)) ([f1e8c8f](https://github.com/exoma-ch/hyrr/commit/f1e8c8f01a9915880dff94751e35ccb2866c135e))
* require explicit density for unknown compounds — no silent fallback ([#354](https://github.com/exoma-ch/hyrr/issues/354)) ([0fc0407](https://github.com/exoma-ch/hyrr/commit/0fc0407cc42b7560598af97b7608244dead162c7))
* update resolve_material call sites for Result + registry param ([#351](https://github.com/exoma-ch/hyrr/issues/351)) ([912f96e](https://github.com/exoma-ch/hyrr/commit/912f96e3b0aaeda02b77bd831853e77b28791d12))
* warn on unknown compound density instead of crashing ([#353](https://github.com/exoma-ch/hyrr/issues/353)) ([07854c8](https://github.com/exoma-ch/hyrr/commit/07854c836bb1da414fb0b326fbff9cf82ae3fa0e))

## [0.12.0](https://github.com/exoma-ch/hyrr/compare/v0.11.3...v0.12.0) (2026-05-27)


### Features

* current profile Phase 2 — preview plot, radio toggle, Python parsers, MCP schema ([#328](https://github.com/exoma-ch/hyrr/issues/328)) ([#347](https://github.com/exoma-ch/hyrr/issues/347)) ([6fad02d](https://github.com/exoma-ch/hyrr/commit/6fad02db064405ddf326b0490297bd6af920ae56))
* **mcp:** define_material tool + density override + remove 5.0 fallback ([#348](https://github.com/exoma-ch/hyrr/issues/348)) ([9b06925](https://github.com/exoma-ch/hyrr/commit/9b069250e40adccb7323df1b72131aff0d461696)), closes [#328](https://github.com/exoma-ch/hyrr/issues/328)


### Tests

* **core:** RED/GREEN regression guard for density override ([#345](https://github.com/exoma-ch/hyrr/issues/345)) ([#349](https://github.com/exoma-ch/hyrr/issues/349)) ([ba9ef54](https://github.com/exoma-ch/hyrr/commit/ba9ef5465f53c0e067598468e422901f703b684d))

## [0.11.3](https://github.com/exoma-ch/hyrr/compare/v0.11.2...v0.11.3) (2026-05-27)


### Bug Fixes

* custom material density now affects implantation depth ([#345](https://github.com/exoma-ch/hyrr/issues/345)) ([021372f](https://github.com/exoma-ch/hyrr/commit/021372fdb44e6487d2c80987bc05ca7f49533aa5))

## [0.11.2](https://github.com/exoma-ch/hyrr/compare/v0.11.1...v0.11.2) (2026-05-27)


### Bug Fixes

* **frontend:** consolidate material popup UX ([#341](https://github.com/exoma-ch/hyrr/issues/341)) ([#342](https://github.com/exoma-ch/hyrr/issues/342)) ([14a57ad](https://github.com/exoma-ch/hyrr/commit/14a57adfc96b538fb8f8493a0299c39eca8d1f7e))

## [0.11.1](https://github.com/exoma-ch/hyrr/compare/v0.11.0...v0.11.1) (2026-05-27)


### Bug Fixes

* **frontend:** move section nav into sticky topbar + match box styling ([#339](https://github.com/exoma-ch/hyrr/issues/339)) ([dc3c26d](https://github.com/exoma-ch/hyrr/commit/dc3c26da3d436fc03c74c0eaeea08e79e514e39c))

## [0.11.0](https://github.com/exoma-ch/hyrr/compare/v0.10.2...v0.11.0) (2026-05-26)


### Features

* **frontend:** add Cloudflare Web Analytics beacon ([#337](https://github.com/exoma-ch/hyrr/issues/337)) ([a660dad](https://github.com/exoma-ch/hyrr/commit/a660dad36f49a15918bcc6150664c37f1912be5c))
* **frontend:** paste URL to load config ([#332](https://github.com/exoma-ch/hyrr/issues/332)) ([#334](https://github.com/exoma-ch/hyrr/issues/334)) ([8d8a2d9](https://github.com/exoma-ch/hyrr/commit/8d8a2d9dec2a8c98edebd632e89c9eb04406e9bf))


### Bug Fixes

* **frontend:** use canonical isotope name format in IsotopePopup ([#330](https://github.com/exoma-ch/hyrr/issues/330)) ([#335](https://github.com/exoma-ch/hyrr/issues/335)) ([eed20d4](https://github.com/exoma-ch/hyrr/commit/eed20d49e5f0ba228233da548db06cea73c3ea4c))

## [0.10.2](https://github.com/exoma-ch/hyrr/compare/v0.10.1...v0.10.2) (2026-05-26)


### Bug Fixes

* **desktop:** update prompt + sticky topbar polish ([#329](https://github.com/exoma-ch/hyrr/issues/329)) ([c49d999](https://github.com/exoma-ch/hyrr/commit/c49d999d8bd82a4621bd6330d9922dbd0f6be4f5))

## [0.10.1](https://github.com/exoma-ch/hyrr/compare/v0.10.0...v0.10.1) (2026-05-26)


### Bug Fixes

* **compute:** normalize state in chain-solver result ([#323](https://github.com/exoma-ch/hyrr/issues/323)) ([#325](https://github.com/exoma-ch/hyrr/issues/325)) ([f77da3f](https://github.com/exoma-ch/hyrr/commit/f77da3f268a3ae30cc3bc26f9fdc5b4ba5a7ff01))
* **frontend:** stack emission bars instead of overlay ([#324](https://github.com/exoma-ch/hyrr/issues/324)) ([#327](https://github.com/exoma-ch/hyrr/issues/327)) ([6ee08aa](https://github.com/exoma-ch/hyrr/commit/6ee08aae8ca93a8c5a53561dd03f465ebbdd3dea))

## [0.10.0](https://github.com/exoma-ch/hyrr/compare/v0.9.1...v0.10.0) (2026-05-26)


### Features

* **frontend:** sticky top bar + share link in save menu ([#320](https://github.com/exoma-ch/hyrr/issues/320)) ([aa252e1](https://github.com/exoma-ch/hyrr/commit/aa252e1c84933753e4a0e1124cae9b161a927a30))
* **mcp:** auto-fetch nuclear data for uvx hyrr-mcp ([#322](https://github.com/exoma-ch/hyrr/issues/322)) ([382e473](https://github.com/exoma-ch/hyrr/commit/382e47381ccc8115dacdbf744e0b778ca0ebbde9))


### Bug Fixes

* **build:** SSoT script for frontend data copy — bundle emissions ([#315](https://github.com/exoma-ch/hyrr/issues/315)) ([#318](https://github.com/exoma-ch/hyrr/issues/318)) ([45dc4ea](https://github.com/exoma-ch/hyrr/commit/45dc4ea29de3742d7fdc4abf089a94731e8bbe75))
* **frontend:** normalize isomeric state in activity table ([#315](https://github.com/exoma-ch/hyrr/issues/315)) ([#316](https://github.com/exoma-ch/hyrr/issues/316)) ([99a59cd](https://github.com/exoma-ch/hyrr/commit/99a59cd30825fb4dca03b4c4ad1976f623444f9b))


### Tests

* **desktop:** golden simulation e2e + workflow_dispatch trigger ([#321](https://github.com/exoma-ch/hyrr/issues/321)) ([8579432](https://github.com/exoma-ch/hyrr/commit/857943228eb13f0d3a57722a7395689f5ceee528))

## [0.9.1](https://github.com/exoma-ch/hyrr/compare/v0.9.0...v0.9.1) (2026-05-24)


### Bug Fixes

* **ci:** mark MCP linux-aarch64 continue-on-error ([#308](https://github.com/exoma-ch/hyrr/issues/308)) ([9f06f0f](https://github.com/exoma-ch/hyrr/commit/9f06f0f16598b612fc5f1e31855f8fb89ea8b5db))
* **ci:** sort JSON keys in MCP parity test (non-deterministic HashMap order) ([#309](https://github.com/exoma-ch/hyrr/issues/309)) ([75ff860](https://github.com/exoma-ch/hyrr/commit/75ff860729de79616f2f1c1dd803ee65786acb95))
* **e2e:** 300s wait for heavy presets (was 110s hardcoded) ([#313](https://github.com/exoma-ch/hyrr/issues/313)) ([65ba9cc](https://github.com/exoma-ch/hyrr/commit/65ba9cc028ec404a67f75f28ae1b8b7eb4837bd5))
* **e2e:** increase preset golden test timeouts for heavy targets ([#311](https://github.com/exoma-ch/hyrr/issues/311)) ([ca9f5b0](https://github.com/exoma-ch/hyrr/commit/ca9f5b01bfc11ae05883f1e2804497fea0f96291))
* **e2e:** split presets into [@preset](https://github.com/preset) (fast) and [@preset-heavy](https://github.com/preset-heavy) ([#314](https://github.com/exoma-ch/hyrr/issues/314)) ([665e0f4](https://github.com/exoma-ch/hyrr/commit/665e0f4f4d241249ce7327ec80256a15fb6da263))
* **e2e:** use test.slow() for heavy preset golden tests ([#312](https://github.com/exoma-ch/hyrr/issues/312)) ([efebd83](https://github.com/exoma-ch/hyrr/commit/efebd83dc1f204c41575cd8912dae127082eb247))
* **frontend:** isomeric state consolidation, emission SSoT, enrichment save ([#302](https://github.com/exoma-ch/hyrr/issues/302) [#303](https://github.com/exoma-ch/hyrr/issues/303) [#304](https://github.com/exoma-ch/hyrr/issues/304) [#305](https://github.com/exoma-ch/hyrr/issues/305)) ([#307](https://github.com/exoma-ch/hyrr/issues/307)) ([368007e](https://github.com/exoma-ch/hyrr/commit/368007e50aac855f5f400ec6ab18a91f0daac2f4))
* **mcp:** deterministic output ordering in MCP tools ([#310](https://github.com/exoma-ch/hyrr/issues/310)) ([dc8341c](https://github.com/exoma-ch/hyrr/commit/dc8341c85f2df781a3c4bcd6add554fb2588f54e))


### Miscellaneous

* **deps:** bump googleapis/release-please-action from 4 to 5 ([#301](https://github.com/exoma-ch/hyrr/issues/301)) ([0131519](https://github.com/exoma-ch/hyrr/commit/0131519fbe74239caa2d266e83548ee2a05cb990))
* **deps:** bump nucl-parquet from `9da1521` to `ee2a8f0` ([#300](https://github.com/exoma-ch/hyrr/issues/300)) ([650175b](https://github.com/exoma-ch/hyrr/commit/650175bdd2aa0e5bea4db5f373790314065d1134))

## [0.9.0](https://github.com/exoma-ch/hyrr/compare/v0.8.0...v0.9.0) (2026-05-22)


### Features

* complete [#219](https://github.com/exoma-ch/hyrr/issues/219) P0 — logX toggle, tab greying, polished plot ([#287](https://github.com/exoma-ch/hyrr/issues/287)) ([d9b36f3](https://github.com/exoma-ch/hyrr/commit/d9b36f3e4ffb3d37b6c75f0eb40cc7478befd5e5))
* continuous β spectrum rendering (Fermi function) ([#290](https://github.com/exoma-ch/hyrr/issues/290)) ([#291](https://github.com/exoma-ch/hyrr/issues/291)) ([8d51cc4](https://github.com/exoma-ch/hyrr/commit/8d51cc4ec9d68eae8efdf39ecb251b4fbdfdce5d))
* **core:** replace ParquetDataStore with nucl-parquet SSoT adapter ([#249](https://github.com/exoma-ch/hyrr/issues/249)) ([e87e2e8](https://github.com/exoma-ch/hyrr/commit/e87e2e80b85d1562a1e0849ebb8323fbc3d3985f))
* **data:** bump nucl-parquet to data-2026.5.1 + wire catima & compound stopping ([#222](https://github.com/exoma-ch/hyrr/issues/222)) ([1f37da8](https://github.com/exoma-ch/hyrr/commit/1f37da811aae6e749d61d54af7dabeea9dbd3b66))
* EmbeddedDataStore — nuclear data baked into binary ([#274](https://github.com/exoma-ch/hyrr/issues/274)) ([#276](https://github.com/exoma-ch/hyrr/issues/276)) ([3f1c869](https://github.com/exoma-ch/hyrr/commit/3f1c869eb82d1e3b4a74d357fcacfe4eae7e3111))
* emission plot tabs + empty state + hover UX ([#284](https://github.com/exoma-ch/hyrr/issues/284)) ([17fd734](https://github.com/exoma-ch/hyrr/commit/17fd734bb6b3cd8c1e015f8f7242cd5310ffb385))
* **frontend:** edit-as-custom for all materials + "New material" row ([#214](https://github.com/exoma-ch/hyrr/issues/214)) ([#220](https://github.com/exoma-ch/hyrr/issues/220)) ([202315f](https://github.com/exoma-ch/hyrr/commit/202315f60c7e4b2e5c457f7e392cda2cc169596d))
* **frontend:** emission tabs (α/β⁻/β⁺/EC/γ) + readParquetRows stack fix ([#201](https://github.com/exoma-ch/hyrr/issues/201)) ([#230](https://github.com/exoma-ch/hyrr/issues/230)) ([f95ef4e](https://github.com/exoma-ch/hyrr/commit/f95ef4efed1aeff932046fcf47d73c65183a3d07))
* **frontend:** per-isotope γ emission table in IsotopePopup ([#201](https://github.com/exoma-ch/hyrr/issues/201)) ([#223](https://github.com/exoma-ch/hyrr/issues/223)) ([8c21ae2](https://github.com/exoma-ch/hyrr/commit/8c21ae2d343cf7749a66cf6d2eb2287b93d1a2ec))
* **frontend:** stack-level γ emission Rate vs E plot ([#219](https://github.com/exoma-ch/hyrr/issues/219)) ([#232](https://github.com/exoma-ch/hyrr/issues/232)) ([32a6b76](https://github.com/exoma-ch/hyrr/commit/32a6b76ee7b8b83f7ca741ccb801a32b1da8c41a))
* **frontend:** sticky beam bar + section nav + table UX ([#236](https://github.com/exoma-ch/hyrr/issues/236), [#237](https://github.com/exoma-ch/hyrr/issues/237), [#238](https://github.com/exoma-ch/hyrr/issues/238)) ([#239](https://github.com/exoma-ch/hyrr/issues/239)) ([010cb45](https://github.com/exoma-ch/hyrr/commit/010cb45a06e0eae277010e24063583fef1370603))
* lazy per-file data fetch for hyrr-mcp (nucl-parquet v0.13.5) ([#275](https://github.com/exoma-ch/hyrr/issues/275)) ([eeaed51](https://github.com/exoma-ch/hyrr/commit/eeaed51e44f93467704fb55e497e5f5ab62040f2))
* NIST compound stopping pipeline ([#292](https://github.com/exoma-ch/hyrr/issues/292)) ([#293](https://github.com/exoma-ch/hyrr/issues/293)) ([9001b07](https://github.com/exoma-ch/hyrr/commit/9001b070b847c04dbb4129b5f23d8f36d5f70b1f))
* offline-first data + data-fetch test coverage ([#263](https://github.com/exoma-ch/hyrr/issues/263), [#264](https://github.com/exoma-ch/hyrr/issues/264)) ([#271](https://github.com/exoma-ch/hyrr/issues/271)) ([beead1f](https://github.com/exoma-ch/hyrr/commit/beead1fecef616cbb59e1d28034c4dcc8304bc38))
* preset golden tests as staging release gate ([#286](https://github.com/exoma-ch/hyrr/issues/286)) ([a55ce96](https://github.com/exoma-ch/hyrr/commit/a55ce96e8bb5294675b2c6b63f0f93adb5716409))
* **python:** delegate DataStore to nucl-parquet DuckDB client ([#257](https://github.com/exoma-ch/hyrr/issues/257)) ([#261](https://github.com/exoma-ch/hyrr/issues/261)) ([c24d1e7](https://github.com/exoma-ch/hyrr/commit/c24d1e7ff8002a5406d5e5903455e31e34ab877c))
* wire EmbeddedDataStore into Tauri, kill resource bundling ([#274](https://github.com/exoma-ch/hyrr/issues/274)) ([#277](https://github.com/exoma-ch/hyrr/issues/277)) ([279a8b8](https://github.com/exoma-ch/hyrr/commit/279a8b84fca3a09b6c96a869726ba4f99ee43406))
* α emission tab + compound stopping (nucl-parquet v0.13.6) ([#289](https://github.com/exoma-ch/hyrr/issues/289)) ([2bc0afc](https://github.com/exoma-ch/hyrr/commit/2bc0afc7f517bed247c9cb1bd0da5b20b277b626))


### Bug Fixes

* **ci:** add nudex_level_gammas.parquet to e2e workflow data copy ([#201](https://github.com/exoma-ch/hyrr/issues/201)) ([#228](https://github.com/exoma-ch/hyrr/issues/228)) ([7306ad0](https://github.com/exoma-ch/hyrr/commit/7306ad0c3747dfc4ea0f2d3ef28c7c244a427c09))
* **compute:** handle beam-stopped-upstream layers without crashing ([#211](https://github.com/exoma-ch/hyrr/issues/211)) ([#212](https://github.com/exoma-ch/hyrr/issues/212)) ([580742b](https://github.com/exoma-ch/hyrr/commit/580742b7c553b6096431ae7fdf3f1ce3844c0df4))
* **compute:** isomeric state mismatch — decay lookup + xs dedup + data-2026.5.4 ([#252](https://github.com/exoma-ch/hyrr/issues/252)) ([#253](https://github.com/exoma-ch/hyrr/issues/253)) ([4d011de](https://github.com/exoma-ch/hyrr/commit/4d011dec58309c87ff9277d52e0a866535cbe8ac))
* **ecard:** use SSoT theme tokens — was unreadable in dark mode ([#213](https://github.com/exoma-ch/hyrr/issues/213) follow-up) ([#216](https://github.com/exoma-ch/hyrr/issues/216)) ([b79b57e](https://github.com/exoma-ch/hyrr/commit/b79b57eb7070ac3ffeb512182a55a49d28f06058))
* emission plot hover — closest mode, prioritize tallest bar ([#285](https://github.com/exoma-ch/hyrr/issues/285)) ([12c13cd](https://github.com/exoma-ch/hyrr/commit/12c13cd80e3c48abdb52af7f05ce1e76c6f9e016))
* emission UX — 511 keV in γ tab, wider hover targets, unified hover ([#283](https://github.com/exoma-ch/hyrr/issues/283)) ([939e415](https://github.com/exoma-ch/hyrr/commit/939e4156268f1b989423cfe0ecadcfc2e04b0b97))
* **emissions:** absolute per-decay intensities from nucl-parquet data-2026.5.2 ([#244](https://github.com/exoma-ch/hyrr/issues/244)) ([99ed669](https://github.com/exoma-ch/hyrr/commit/99ed669ec047ca95bbb9a484fc71aad1acc4abed))
* **enrichment:** natural-fill partial vectors + guard Apply on invalid sums ([#217](https://github.com/exoma-ch/hyrr/issues/217)) ([#218](https://github.com/exoma-ch/hyrr/issues/218)) ([76ea62e](https://github.com/exoma-ch/hyrr/commit/76ea62e85f5ba94f13cb79b12c9419fa339b4f49))
* **errors:** layer-attributed StoppingError + honest ECard + kill NaN panic ([#213](https://github.com/exoma-ch/hyrr/issues/213)) ([#213](https://github.com/exoma-ch/hyrr/issues/213)) ([0cdc17b](https://github.com/exoma-ch/hyrr/commit/0cdc17b4dcfb35fdc835dd0a820a5c41bedc9957))
* **frontend:** iOS mobile bugs — dose display, input overflow, unit UX ([#233](https://github.com/exoma-ch/hyrr/issues/233) [#234](https://github.com/exoma-ch/hyrr/issues/234) [#235](https://github.com/exoma-ch/hyrr/issues/235)) ([#241](https://github.com/exoma-ch/hyrr/issues/241)) ([aff7bc3](https://github.com/exoma-ch/hyrr/commit/aff7bc3bd23090532c1a50362d1d2f801675eee0))
* **frontend:** readParquetRows stack overflow + emission threshold grouping ([#201](https://github.com/exoma-ch/hyrr/issues/201)) ([#229](https://github.com/exoma-ch/hyrr/issues/229)) ([ead5293](https://github.com/exoma-ch/hyrr/commit/ead529339528c6b018a7976e50dc664d78ce70b7))
* **frontend:** remove heavy-ion projectiles until XS routing wired ([#267](https://github.com/exoma-ch/hyrr/issues/267)) ([3c2426e](https://github.com/exoma-ch/hyrr/commit/3c2426eb94b2f7b20e70be0606584ee8a12fd040)), closes [#266](https://github.com/exoma-ch/hyrr/issues/266)
* **frontend:** β⁺ endpoint energy + γ relative intensity labeling ([#240](https://github.com/exoma-ch/hyrr/issues/240), [#242](https://github.com/exoma-ch/hyrr/issues/242)) ([#243](https://github.com/exoma-ch/hyrr/issues/243)) ([c94f822](https://github.com/exoma-ch/hyrr/commit/c94f822b0cfe1bc2c21b129e992b270dd73a545f))
* include 511 keV annihilation in emission plot + preset golden tests ([#282](https://github.com/exoma-ch/hyrr/issues/282)) ([062ba72](https://github.com/exoma-ch/hyrr/commit/062ba72772f827dfc4ce5451af0fd92f3f106d3e))
* **python:** normalize 'g'→'' decay lookup + xs dedup in DataStore ([#254](https://github.com/exoma-ch/hyrr/issues/254)) ([#255](https://github.com/exoma-ch/hyrr/issues/255)) ([5765679](https://github.com/exoma-ch/hyrr/commit/57656797f56a1388296aa939f9bfec4e4fe6a174))
* read bundled data in-place, eliminate download/cache flow ([#272](https://github.com/exoma-ch/hyrr/issues/272)) ([2207798](https://github.com/exoma-ch/hyrr/commit/22077983e16fe8972e978d59592bab2fa4a69f83)), closes [#264](https://github.com/exoma-ch/hyrr/issues/264)
* SSoT for DEFAULT_LIBRARY via hyrr.json ([#269](https://github.com/exoma-ch/hyrr/issues/269)) ([#270](https://github.com/exoma-ch/hyrr/issues/270)) ([d0659d5](https://github.com/exoma-ch/hyrr/commit/d0659d5f641158e985725791cbfe5b8631c81849))
* switch default library to tendl-2023-iso (isomeric splitting) ([#265](https://github.com/exoma-ch/hyrr/issues/265)) ([#268](https://github.com/exoma-ch/hyrr/issues/268)) ([c737571](https://github.com/exoma-ch/hyrr/commit/c737571924e4d815c48f1f5570b814f9d21c70dd))
* WASM parseFormula static→instance, preset golden tests ([#279](https://github.com/exoma-ch/hyrr/issues/279)) ([#280](https://github.com/exoma-ch/hyrr/issues/280)) ([1dd683f](https://github.com/exoma-ch/hyrr/commit/1dd683f355a3de16d8e3898d4ffa4361a532a317))
* wire compound stopping (nucl-parquet v0.13.6) + emission plot polish ([#288](https://github.com/exoma-ch/hyrr/issues/288)) ([b4531a0](https://github.com/exoma-ch/hyrr/commit/b4531a04cd821784410259d577e411370a9fc619))
* β spectrum sum envelope on shared interpolated grid ([#294](https://github.com/exoma-ch/hyrr/issues/294)) ([08e2653](https://github.com/exoma-ch/hyrr/commit/08e2653545d5fab636dee87508c00098fc358692))


### Tests

* **compute:** matrix emission test — 7 γ isotopes + 6 decay channels ([#201](https://github.com/exoma-ch/hyrr/issues/201)) ([#231](https://github.com/exoma-ch/hyrr/issues/231)) ([575dcfb](https://github.com/exoma-ch/hyrr/commit/575dcfb62d3abcbc98443803059be96631b484e8))
* **desktop:** Tauri WebDriver smoke tests ([#188](https://github.com/exoma-ch/hyrr/issues/188)) ([4bca814](https://github.com/exoma-ch/hyrr/commit/4bca8141bdbf249ca1b423582c85d963140f58ff))
* EmbeddedDataStore integration tests + CI ([#274](https://github.com/exoma-ch/hyrr/issues/274)) ([#278](https://github.com/exoma-ch/hyrr/issues/278)) ([e7433be](https://github.com/exoma-ch/hyrr/commit/e7433bead2aa801f1efb6c3e1c82a34df70c5b9c))


### Miscellaneous

* **deps:** bump actions/cache from 4 to 5 ([#226](https://github.com/exoma-ch/hyrr/issues/226)) ([dc4d7ae](https://github.com/exoma-ch/hyrr/commit/dc4d7ae966fa530ea3c89c7e19ced726c0bb6256))
* **deps:** bump actions/setup-node from 4 to 6 ([#225](https://github.com/exoma-ch/hyrr/issues/225)) ([7b335d2](https://github.com/exoma-ch/hyrr/commit/7b335d2ee2862bd9304730ad44482c68b95c8347))
* **deps:** bump hyparquet-writer from 0.14.0 to 0.15.1 in /frontend ([#227](https://github.com/exoma-ch/hyrr/issues/227)) ([91d3f86](https://github.com/exoma-ch/hyrr/commit/91d3f86151e04b3eb3735cb1d6fccc4c5a2f8847))
* devendor data/parquet/ from git — download on demand ([#259](https://github.com/exoma-ch/hyrr/issues/259)) ([#262](https://github.com/exoma-ch/hyrr/issues/262)) ([0ed4ff4](https://github.com/exoma-ch/hyrr/commit/0ed4ff4725538774d9ba773235f8b0f07a7d4e91))
* remove dead limited-mode code ([#273](https://github.com/exoma-ch/hyrr/issues/273)) ([8e31d2e](https://github.com/exoma-ch/hyrr/commit/8e31d2ebcf2f90a350fe8b9358044a76ae0e5a19))


### CI

* copies decay_detailed.parquet in deploy, promote, and e2e workflows. ([f95ef4e](https://github.com/exoma-ch/hyrr/commit/f95ef4efed1aeff932046fcf47d73c65183a3d07))

## [0.8.0] — 2026-05-13

The **Local-first** release. Three architectural shifts let HYRR run as an
honest offline-first tool: a lazy nuclear-data cache that no longer ships
the 380 MB blob inside the installer, a typed-error chain that surfaces
recoverable failures to the user instead of crashing the splash, and a
real Tauri desktop app with auto-updater + cross-platform Playwright
coverage. The MCP server is now a first-class entry point distributed via
`uvx`. ~80 merged PRs since v0.7.0.

### Added

- **Tauri desktop app** with auto-updater (minisign-signed, build attestations) and external-link routing through `tauri-plugin-opener`. Ships as `.dmg` / `.msi` / `.deb` / `.AppImage` via GitHub Releases (#125, #129, #116).
- **Lazy nuclear-data cache** — desktop and CLI fetch `nucl-parquet-data-v{V}.tar.zst` from GitHub Releases on first launch into `~/.hyrr/nucl-parquet/v{V}/`, with atomic install semantics (sentinel-last, partial-dir promotion, flock + process-mutex serialisation). The installer no longer bundles 380 MB of data (#52, #117, #121, #122, #123).
- **Splash-screen recovery UI** — when `ensure_data` fails on first launch, a typed `FetchErrorCard` surfaces Retry / Open URL / Install from local tarball / Use bundled-data-only with the actual GitHub URL inline. Push-based `FetchProgress` events drive a live progress bar (`Connecting → Downloading → Extracting → Verifying`) so users can distinguish a slow download from a hang (#118, #160).
- **Staging deploy slot** — every push to main lands at `https://exoma-ch.github.io/hyrr/tst/`. Promotion to prod (`/hyrr/`) is manual workflow_dispatch, gated by a post-deploy smoke test. Broken main commits can no longer reach the user-facing URL (#156, #187).
- **MCP server as first-class entry** — `hyrr-mcp` PyPI package distributed via `uvx`, wheel matrix across Linux/macOS/Windows, trusted PyPI publishing. Library override via `--library` / `HYRR_LIBRARY`, parity coverage with the Python core, stopping-only fast path for `get_stack_energy_budget` (#67, #70, #71, #73, #79, #80, #82, #83, #84).
- **Material-popup unified redesign** — rows-based DefineForm with paste-formula auto-detection (single-formula / mass-mixture / mole-mixture / by-atom), standalone PeriodicTable component, per-row enrichment "E" button, balance toggle, mode-switch undo strip, supplier links for enriched isotopes, gas-target catalog entries (#64 epic, #75, #77, #86, #88, #92, #93, #94, #95, #96, #97, #98).
- **PWA installability** — manifest + iOS meta tags for "Add to Home Screen" on the web frontend (#34, #115).
- **Bug-report modal Title field** with auto-derivation from description; "Open on GitHub" button greyed-with-hover-info until description has content; layer-stack clear-all × button (#144, #161-fix, #182).
- **CLI progress bar** — `hyrr fetch-data` shows tqdm progress on TTY, one-line-per-stage on non-TTY. PyO3 binding accepts an optional Python callable that's invoked with throttled `FetchProgress` events (#172).
- **Plot/table export** — CSV export on every plot, parquet session save/restore, save-icon dropdown unifying the surface.
- **Greek-symbol decay/reaction labels** in the activity table (α, β−, β+, EC, γ) for compactness (#131).
- **Display-layer value clamping** with configurable thresholds so the activity table doesn't render `1e-42 Bq` rows (#130).
- **RNP%-multi-select** with searchable picker and per-curve max markers; grouped/per-layer toggle on the activity table.
- **Test infrastructure tiers** — Tier 1 Rust projectile-matrix smoke (`core/tests/projectile_matrix.rs`), Tier 2 cross-platform Playwright E2E across Ubuntu/Windows/macOS + Linux distros (Arch/Fedora/Debian), `@testing-library/svelte` component-render coverage. 515 frontend tests, 30 Rust core lib tests (#148, #136, #169, #178).
- **CI guards** — `cargo check -p hyrr-py` (catches PyO3 binding drift), `data-fetch-ssot` grep guard (no hardcoded release URLs in docs/code), `supplier-catalog-freshness` warning at >90 days stale (#180/#181/#183).

### Changed

- **BREAKING — nucl-parquet wire format split (CalVer data, SemVer code).** The release URL changed from `v{V}/nucl-parquet-data-v{V}.tar.zst` to `data-{V}/nucl-parquet-data-{V}.tar.zst`, and `DATA_VERSION` now reads from `nucl-parquet/data/catalog.json::data_version` instead of `pyproject.toml::version`. Latest data ships as `data-2026.5.0`. ³He stopping is no longer a separate table — `He3STAR.parquet` is gone and ³He routes through ASTAR with velocity scaling at lookup time. Existing caches at `~/.hyrr/nucl-parquet/v{V}/` are still readable (cache layout unchanged). (#196)
- **nucl-parquet** bumped to v0.10.x — TENDL-2025 default across all surfaces, hi-xs-prod heavy-ion data, catima-derived stopping for Z≥6 projectiles (#91, #113, #127, #128).
- **Default cross-section library** is `tendl-2025` everywhere; configurable via `--library`, `HYRR_LIBRARY`, or `DataStore(library=…)` (#91).
- **Typed `StoppingError` / `FetchError` chains** replace `Result<_, String>` at all IPC boundaries. Frontend `parseStoppingError` / `parseFetchError` decode the JSON wire format into discriminated unions; recovery cards render variant-specific titles (`Couldn't download nuclear data — HTTP 404`, `Energy out of table range`, etc.) (#142, #150).
- **Single source of truth for data-fetch constants** — `hyrr_core::data_fetch::{release_url,tarball_filename,cache_root_pattern,data_version}` plus mirror Tauri commands and frontend `data-fetch-meta.ts` getters. No more concrete `v0.10.0/nucl-parquet-data-v0.10.0.tar.zst` literals scattered across docs/CI/code (#118-contract, #157).
- **Shared `FetchProgressThrottle`** — desktop and PyO3 binding both use one canonical 100 ms / 256 KiB combinator in `hyrr_core::data_fetch`. Replaced two ~60-LOC inline copies (#180).
- **Activity-plot legend behaviour** — selection persists across `Plotly.react()` calls via a `Map<string, boolean | "legendonly">` instead of being clobbered on every re-render.
- **Inline Octicon SVG paths replaced by `lucide-svelte` components** across `HeaderBar`, `ActivityTableEnhanced`, `SaveMenu`, `IsotopePopup`, `Modal`. Tree-shaken per-icon, no more hand-pasted `<path d="…"/>` blobs. GitHub mark stays inline (lucide ships no brand glyphs) (#204).
- **`init_data_store` and `FetchErrorPayload`** redact `$HOME` to `~/…` before crossing the IPC boundary, so bug-report attachments don't leak the OS username (#173, #176).

### Fixed

- **Plotly v3 axis-title API drift** — every plot's axis labels were missing on /tst after the Plotly v3 upgrade because v3 requires `xaxis.title.text` (object) instead of `xaxis.title` (string). All plots ported to the object form (#197/#199).
- **K/L/M-shell EC duplication** — the isotope-popup decay-mode chip and decay chain displayed `EC + EC + EC + β⁺` from the K/L/M shell-resolved entries; aggregated to a single `EC` bucket with branching summed. The activity-table Reaction column applies the same collapse for the per-isotope notation (e.g. `⁵¹Mn(EC), ⁵¹Mn(β⁺)` instead of four entries) (#198/#202/#205).
- **Activity-table cells in scientific notation below 1 Bq** — `fmtActivity` / `fmtYield` / `fmtDoseRate` only had k/M/G/T tiers, so a `7.63e-6 Bq` cell read as exponential. Refactored around a single `fmtSI` helper extending the SI prefix table down to p-Bq / p-Sv-h⁻¹ (#205).
- **Activity-plot save icon mid-toolbar** — wrapped the save+clear cluster in a right-aligned span so it hugs the right edge, matching every other plot's layout (#205).
- **Cog/filter UX consolidation in activity table** — the `~0` / `0` / `all` chip group moved into a cog dropdown adjacent to the save button. Indicator label `~0` → `<X`. `formatWithThresholdEx` short-circuits true zero to the unit-aware zero label in every mode so only sub-threshold non-zero cells get mode-dependent rendering (#204).
- **Plots piecewise-linear on the high-energy front of every layer** — the depth-profile grid was linear-in-energy, which concentrates depth points at the Bragg peak (large dE/dx → small Δd) and starves the beam-entry side (small dE/dx → large Δd). `generate_depth_profile` now resamples to a uniform-in-depth grid via dE/dx back-solve; per-layer adjacent Δd varies by <2 % regardless of where on the curve. New `depth_grid_is_uniform_in_depth` regression test; conservation invariant `depth_rate_integrates_to_production_rate` unchanged (#206 stop-gap → #210 proper fix, closes #208).
- **macOS desktop builds aborted in attest step** — `actions/attest-build-provenance@v4` recurses into directories, and a Tauri `.app` is a directory with >1024 contained files. Filter step strips `.app` entries from the subject list before attest; the `.dmg` and `.tar.gz`/`.sig` pair cover the contents transitively (#207).
- **Staging deploy missed wasm-only/core-only PRs** — `deploy-frontend.yml` only watched `frontend/**` paths, but the deployed bundle includes WASM built from `wasm/**` linking `core/**`. Added `wasm/**`, `core/**`, `Cargo.lock` to the path filter (#209).
- **Heavy-ion crashes** — `compute_stack` for C-12 / O-16 / Ne-20 / Si-28 / Ar-40 / Fe-56 hit a catima parquet-key mismatch (`catima_O-16` vs `catima_O16`); fixed by stripping the dash in the source-key construction and adding a typed-error path when the table lookup misses (#137-fix, #141, #142, #150, #161).
- **Stale results displayed after compute failure** — the previous successful run's stats no longer mask a freshly-failed run; both `setResultErrored` and `setComputeError` fire from the scheduler's single funnel (#143).
- **Activity clamp too loose** — `R × λt` allowed 40× spurious peaks mid-irradiation; replaced with the analytical saturation envelope `R × (1 − exp(−λt_irr))` (closes #55).
- **Activity plot 1 trace per layer × N layers** — aggregated by isotope name; the per-layer view is now opt-in via a toggle (closes #54).
- **Save-menu invisible** — dropdown was clipped by `overflow: hidden` on `.header-bar`; window-event handler had a flip race with the toggle button.
- **External links no-op in WKWebView** — bug-report and GitHub buttons now route through `tauri-plugin-opener` instead of `window.open` (#129).
- **Stale WASM binary** — checked-in `hyrr_wasm_bg.wasm` was 5 weeks behind the Rust source; rebuild + CI guard so future drift is caught (#152).
- **Tarball extract refuses non-regular entries** — symlinks, hardlinks, devices, etc. are rejected before extraction (#122).
- **Decay chain solver** — bypassed broken matrix-exp on chains of mixed half-lives, falls back to analytical Bateman per isotope (closes #58).
- **`init_data_store` IPC error** no longer leaks the resolved data dir's absolute path (#176).
- **Bug-report "Open on GitHub" silently failed** when description was empty — getSerializableConfig fallthrough fixed, plus an enable-gate with hover info on the disabled state (#137-related).

### Removed

- **Bundled 380 MB nuclear data from the installer** — replaced by lazy fetch on first launch (#52, #117). Air-gapped installs use `hyrr fetch-data --offline-bundle <path>`.
- **Duplicate physics implementations in Python and TypeScript** — `hyrr_core` Rust crate is now the SSoT for stopping/compute/Bateman; PyO3 + WASM bindings expose it. The legacy Python core and TS port survived as compat shims through earlier releases; in 0.8 they're gone (#67-era rust-ssot-cutover).
- **`feat/v0.7.1-batch`-era inline material picker** — superseded by the unified material-form redesign (#92, #98).

### Security

- **Tarball extraction** rejects symlinks/hardlinks/devices, eliminating a class of path-traversal vectors when installing from an untrusted offline bundle (#122).
- **Tauri auto-updater** signs releases with minisign + verifies build attestations; the updater refuses unsigned releases (#116, #125).
- **IPC payload redaction** — `$HOME` stripped from cache paths in error payloads so bug-report attachments don't carry the OS username (#173, #176).

### Infrastructure

- **CI matrix** now covers: Python (pyright + ruff + pytest), Rust projectile-matrix (Tier 1), cross-platform Playwright E2E (Tier 2: Ubuntu 22/24, Windows 22/25, macOS 15, Linux distros Arch/Fedora/Debian), `cargo check -p hyrr-py` (PyO3 drift), data-fetch SSoT grep, supplier-catalog freshness, docs build validation. ~15 jobs per PR depending on touched paths.
- **Cross-platform Tauri builds** ship signed installers via GitHub Releases; minisign keys + auto-updater plumbing wired end-to-end.
- **Staging slot** at `/hyrr/tst/` decouples broken main commits from the user-facing prod URL. Manual promotion via workflow_dispatch.

## [0.7.0] — 2026-03-27

### Changed

- **nucl-parquet** bumped to v0.9.0 — isomeric state data (699 metastable nuclides), state-scoped radiation lines, updated dose constants covering isomeric pure-beta emitters

### Added

- **IsotopePopup: XS data loading** — cross-section parquet files are now lazily loaded when the popup opens, restoring XS plots, depth plots, theory/real toggle, and compare isotope dropdown
- **IsotopePopup: Real depth production rates** — "Real" mode now computes σ(E(x)) × abundance × number density × beam flux in the frontend, independent of backend; shows actual production rate (atoms/s/cm) per layer with correct density and isotopic scaling
- **Download links with OS detection** — Help modal shows an OS-aware "Download for macOS/Windows/Linux" button; footer shows "Desktop app (macOS)" etc.
- **Playwright e2e tests** for isotope depth plot theory/real toggle (3 tests)

### Fixed

- **IsotopePopup blank on open** — `getCrossSections()` returned empty because the scheduler's DataStore never had XS parquet files loaded; fixed by calling `ensureMultipleCrossSections()` before reading
- **Real mode depth plot was empty** — Rust WASM engine doesn't output `depth_production_rates`; replaced with frontend-computed rates using XS channels + depth preview + material composition
- **Playwright webServer timeout** — increased from 60s to 120s (build alone takes ~35s)

## [0.6.1] — 2026-03-24

### Added

- **Repeating layer groups** — wrap any set of layers into a group and repeat them N times or until beam energy drops below a threshold; groups are first-class in the UI and persist across reloads
- **Group persistence** — groups survive URL hash sharing and session tab restore (encoded as `{g:true, ...}` in compact config format v2)
- **Shared isotope filter bar** — filter panel extracted above both the activity plots and the activity table; filter state is shared between both views
- **Simulation mode toggle** — Auto/Manual button next to beam properties with live status dot (idle / busy / ready / error)
- **Undo/redo** — Cmd+Z / Cmd+Shift+Z (50-deep snapshot stack); also accessible via keyboard while any text field is not focused
- **Clear history** — "Clear all" button in the history panel with inline Yes/No confirmation
- **Leading-decimal thickness** — thickness inputs now accept `.2mm`, `.5µm`, etc.

### Fixed

- **Li-5 phantom activity (#40)** — matrix exponential residuals for ultra-short half-lives (t½ < 1 µs) are replaced with analytical Bateman solution, eliminating spurious MBq readings
- **Production depth plot zero-crossing (#41)** — isotopes that don't appear in a layer now produce a clean zero segment rather than a diagonal artifact
- **Material picker / enrichment context (#42)** — popup handlers now use internal item indices (not expanded flat layer indices), so clicking enrichment on a CaO layer no longer defaults to Al
- **GitHub Actions storage** — Rust build caches only saved on tag releases; Pages and dist artifacts have explicit retention limits; benchmark job scoped to `src/**` changes

## [0.5.0] — 2026-03-18

### Added

- **Desktop app** — offline-capable native app via Tauri v2 for air-gapped machines; bundles all nuclear data (~68 MB Parquet); builds for Windows (.msi/.exe), macOS (.dmg, ARM64 + x64), and Linux (.deb/.AppImage)
- **Tauri CI workflow** (`.github/workflows/tauri-build.yml`) — cross-platform build + GitHub Release upload on `v*` tags
- **Conditional base path** — frontend `vite.config.ts` uses `./` for Tauri, `/hyrr/` for web
- **Logo asset imports** — `HeaderBar` and `WelcomeScreen` use Vite `?url` imports for base-path-independent logo resolution

### Fixed

- **Projectile select styling** — added `appearance: none` with custom chevron for consistent rendering across Chrome and macOS WebKit (Tauri webview)

## [0.4.0] — 2026-03-16

### Added

- **`@hyrr/compute` shared package** — extracted physics engine into `packages/compute/` as an npm workspace, consumable by both frontend and Node.js tools
- **MCP server** (`mcp/`) — agent-driven irradiation analysis via Model Context Protocol; tools: `simulate`, `list_materials`, `get_cross_sections`, `get_decay_data`, `compare_results`; resources: `hyrr://libraries`, `hyrr://elements`
- **NodeDataStore** — filesystem-backed Parquet data store for CLI/MCP usage (`@hyrr/compute/node`)
- **Isotope popup: Theory/Real toggle** — depth plot switches between raw σ(E(x)) and actual production rate (atoms/s/cm) from simulation, fully layer/density/abundance-aware

### Changed

- **Production depth plot** moved directly below stopping profile for visual continuity
- **Frontend imports** rewired from local `compute/` to `@hyrr/compute` workspace package
- **Service worker cache versioning** — cache name now includes app version; version bump automatically purges old caches

## [0.3.3] — 2026-03-16

### Fixed

- **URL sharing** — shared config URLs now correctly override session restore instead of being silently overwritten (#29)
- **Preset loading** — clicking a preset (e.g. Tc-99m) now always triggers simulation, even if the same config was previously loaded (#30)
- **Material picker auto-focus** — search input is focused on open so users can type immediately; pressing Enter selects the first result with proper casing (#31)

### Changed

- **Mobile responsive styles** — added `@media` breakpoints at 640px/1024px across 9 components: full-width history drawer, grid beam config, larger touch targets, sticky table columns, hidden low-priority columns, full-screen modals on phone

## [0.3.1] — 2026-03-13

### Security

- **Worker origin enforcement** — disallowed origins rejected server-side (not just CORS headers)
- **Cloudflare Turnstile** (invisible CAPTCHA) required before anonymous issue creation via worker
- **Upload content-type validation** — allowlist `image/jpeg`, `image/png`, `image/webp` with magic byte verification; `Content-Disposition: inline` forced on served images
- **Input limits** — title ≤200 chars, body ≤10,000 chars, labels restricted to `["bug"]`
- **Config URL validation** — shape validation after decode (max 20 layers, finite numeric fields)
- **History import validation** — validates each entry before storing; skips malformed entries

### Changed

- **Bug report modal** — three-button flow: Cancel / Open on GitHub / Submit. Email required only for worker submit; GitHub route uses the user's own session
- **nucl-parquet** bumped to v0.3.4

## [0.2.0] — 2026-03-13

### Added

- **Bug report modal** with GitHub App integration — users can submit issues with screenshots directly from the app, no GitHub account required
- **Screenshot upload** with client-side downsampling (max 1280px, JPEG 80%) to Cloudflare R2
- **HYRR logo** in header bar and browser favicon (isometric wave visualization)
- **Session tabs** with Chrome-style tab bar for managing multiple configurations
- **Compare isotopes** in IsotopePopup — multi-channel XS overlay with superscript notation
- **Reaction notation column** in activity table
- **Custom materials** editor with cstm/enr badges on layer cards
- **RNP% calculation** in activity table (relative contribution per layer)
- **3D geometry module** — STEP import, tetrahedral meshing, ray casting (Python)
- **Energy straggling** and beam profile support (Python)
- **nucl-parquet** as external data source with configurable library selection

### Changed

- Repo moved from `MorePET/hyrr` to `exoma-ch/hyrr` — all links updated
- Bug report body now compact: reproducible config URL + one-line summary instead of full JSON dump
- Session tab heights reduced for cleaner header
- XS scaled by atomic abundance (stoichiometry x isotopic fraction)

### Fixed

- Numerically stable decay chain solver
- IsotopePopup XS plot rendering and deep config tracking
- Plotly reactivity and MaterialPopup crash
- Clamp unrealistic activity for long-lived isotopes
- Removed broken TENDL links (PSI server down)

### Removed

- Legacy SQLite build-db command
- Unused frontend components

## [0.1.0] — 2025-12-01

### Added

- Initial frontend: Svelte 5 + TypeScript + Vite
- Pure TypeScript physics compute (ported from Python)
- Lazy-loaded Parquet nuclear data via hyparquet
- URL hash config sharing
- IndexedDB history
- Layer stack builder, beam configuration, activity table
- IsotopePopup with cross-section and activity plots
