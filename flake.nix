{
  description = "hyrr — Hierarchical Yield and Radionuclide Rates";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    # Shared code-quality / observability gates (#159). `follows` keeps one
    # nixpkgs eval (shared store/cache).
    guardrails.url = "github:gerchowl/guardrails";
    guardrails.inputs.nixpkgs.follows = "nixpkgs";
    # crane: hermetic, cached Rust builds for the `nix flake check` gates
    # (#484). Vendors deps from Cargo.lock so `cargo {test,clippy,fmt}` run
    # byte-identically local and in CI — no `dtolnay/rust-toolchain` +
    # `Swatinem/rust-cache` per-job network dance.
    crane.url = "github:ipetkov/crane";
    # Pinned nucl-parquet data + Rust client at the submodule rev (#484). The
    # data tests + build.rs (`data/catalog.json` → HYRR_DATA_VERSION) need it as
    # a *fixed input* (a hermetic check can't `git submodule update`). This is
    # the same tree the `nucl-parquet` submodule points at; bump both in
    # lockstep. flake=false → consumed as a plain source path.
    nucl-parquet = {
      url = "github:exoma-ch/nucl-parquet/00a9efe5eab0c83a9edad8026c43fe246779e2b8";
      flake = false;
    };
  };

  outputs = { nixpkgs, flake-utils, guardrails, crane, nucl-parquet, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # guardrails: the gate binaries (`guardrails-<name>`) + the devShell
        # wrapper. `mkDevShell`'s shellHook auto-installs the prek hooks the
        # worktree-safe way — via `git rev-parse --git-path hooks`, NOT the
        # `[ -d .git ]` check that silently fails in linked worktrees (where
        # `.git` is a file) — and self-bootstraps the shell, so commits from any
        # worktree / subagent / plain shell run the gates instead of erroring on
        # missing `guardrails-*` binaries. The `ci` shell stays a bare mkShell
        # (it runs `prek run --all-files` explicitly, so it needs the gate
        # binaries but not the auto-install, and stays lean for fast CI). (#159)
        inherit (guardrails.lib.${system}) gates mkDevShell;
        isDarwin = pkgs.stdenv.isDarwin;
        isLinux = pkgs.stdenv.isLinux;

        # Pinned interpreter so local == CI. uv is told to use this exact
        # system interpreter (UV_PYTHON below) instead of downloading a
        # generic-linux build that NixOS can't execute.
        python = pkgs.python312;

        # Runtime libs that Python manylinux wheels (numpy/polars/scipy) link
        # against but NixOS doesn't put on the default loader path. Without
        # these, `import numpy` fails with "libstdc++.so.6 / libz.so.1 cannot
        # open shared object file".
        pyRuntimeLibs = with pkgs; [
          stdenv.cc.cc.lib # libstdc++.so.6, libgcc_s.so.1
          zlib             # libz.so.1
        ];

        # Tauri 2 / WebKitGTK system libs (Linux desktop builds only).
        tauriLibs = with pkgs; [
          webkitgtk_4_1
          gtk3
          glib-networking
          libsoup_3
          openssl
          librsvg
          gdk-pixbuf
          cairo
          pango
          atk
          dbus
        ];

        # Lean Python/lint toolchain — single source of truth for ruff,
        # pyright, uv, and the interpreter. Used by CI lint + test jobs via
        # `nix develop .#ci` so CI tooling versions match local exactly.
        pythonTooling = with pkgs; [ uv ruff pyright python ];

        # Tools the prek hooks invoke directly (the `language: system` hooks in
        # .pre-commit-config.yaml: typos, typstyle, just-fmt, check-lockfiles)
        # plus git. ruff + pyright come from pythonTooling. All from nixpkgs so
        # they run on NixOS (prek's downloaded generic-linux hook binaries hit
        # the stub-ld wall otherwise). `prek` is split out because the `default`
        # shell gets it (and the gates) from mkDevShell's toolbelt — adding it
        # again would double-source it on PATH.
        hookExtras = with pkgs; [ typos typstyle just git ];
        # `ci` is a bare mkShell, so it lists prek + the gate binaries explicitly.
        hookTooling = with pkgs; [ prek ] ++ hookExtras;

        # uv env: use the pinned nix interpreter, never a downloaded one.
        uvEnv = {
          UV_PYTHON = "${python}/bin/python3.12";
          UV_PYTHON_PREFERENCE = "only-system";
        };

        mkLibPath = libs:
          pkgs.lib.optionalString isLinux (pkgs.lib.makeLibraryPath libs);

        # ══ Hermetic Rust checks (crane, #484) ═══════════════════════════
        # Lift the ci.yml cargo jobs (rust-projectile-matrix, embedded-data-
        # store, mcp-tools, hyrr-py-check, data-fetch-integration) into
        # `nix flake check` derivations so they run byte-identically on a
        # laptop and in CI — deps vendored from Cargo.lock, toolchain pinned by
        # flake.lock, no per-job Nix-reinstall + rust-cache dance.
        craneLib = crane.mkLib pkgs;

        # Each crate is standalone (no cargo workspace; `py` is excluded for the
        # PyO3 extension-module link issue), so crane builds each with its own
        # Cargo.toml/lock at the source root — the layout cargo + vendored git
        # deps expect. core/build.rs reads two siblings of core/:
        # `../nucl-parquet/data/catalog.json` (→ HYRR_DATA_VERSION) and
        # `../hyrr.json` (→ default library). We materialise those in the build
        # sandbox's parent dir (`..` of sourceRoot) — nucl-parquet (~650 MB) as
        # a symlink (referenced store path, never copied), hyrr.json copied in.
        nuclData = "${nucl-parquet}/data";
        provisionSiblings = ''
          ln -sfn ${nucl-parquet} ../nucl-parquet
          install -m644 ${./hyrr.json} ../hyrr.json
        '';
        # py path-deps ../core, which itself path-deps ../nucl-parquet and reads
        # ../hyrr.json — so the py build sandbox needs all three siblings.
        provisionCoreSibling = ''
          ${provisionSiblings}
          cp -r ${craneLib.cleanCargoSource ./core} ../core
          chmod -R u+w ../core
        '';

        commonRustArgs = {
          src = craneLib.cleanCargoSource ./core;
          pname = "hyrr-core";
          version = "0.0.0";
          strictDeps = true;
          preConfigure = provisionSiblings;
          nativeBuildInputs = with pkgs; [ pkg-config ];
        };
        # Deps (incl. the guardrails-trace git dep, fetched by rev) built once
        # and reused by every check. Default features keep this lean; checks
        # that need `mcp`/`embed-data` rebuild just those extra crates on top.
        cargoArtifacts = craneLib.buildDepsOnly commonRustArgs;

        # ── Pure-source guards (lifted verbatim from ci.yml) ─────────────
        # No toolchain/network — just grep/node over the tree. Trivially
        # hermetic, so they belong in `nix flake check` rather than a runner job.
        mkGuard = name: script:
          pkgs.runCommandLocal "check-${name}" { } ''
            cd ${./.}
            ${script}
            touch $out
          '';
      in
      {
        devShells = {
          # ── Lean CI/Python shell ─────────────────────────────────────
          # Python toolchain + prek + the gate binaries — fast to realize (no
          # WebKitGTK / Rust / Node / cargo-* toolbelt), used by CI lint + test
          # jobs. CI invokes `prek run --all-files` explicitly, so it needs the
          # gate binaries but NOT mkDevShell's auto-install/self-bootstrap — which
          # is why this stays a bare mkShell rather than mkDevShell.
          ci = pkgs.mkShell ({
            packages = pythonTooling ++ hookTooling ++ [ gates ];
            LD_LIBRARY_PATH = mkLibPath pyRuntimeLibs;
          } // uvEnv);

          # ── Full dev shell (guardrails mkDevShell) ───────────────────
          # This is the shell direnv / `nix develop` / subagents enter. Going
          # through mkDevShell means its shellHook auto-wires the prek hooks
          # worktree-safely and self-bootstraps the env, so a commit/push from
          # any worktree or subagent runs the gates without a manual `nix
          # develop` (#159). prek + the gates come from the toolbelt — don't list
          # them in `extra`.
          default = mkDevShell {
            inherit pkgs;
            name = "hyrr-dev";
            extra = with pkgs; pythonTooling ++ hookExtras ++ [
              # ── Rust ──────────────────────────────────────────────
              rustc
              cargo
              clippy
              rustfmt
              rust-analyzer

              # ── WASM ──────────────────────────────────────────────
              wasm-pack
              wasm-bindgen-cli

              # ── Node / frontend ───────────────────────────────────
              nodejs_22

              # ── Build tools ───────────────────────────────────────
              pkg-config
              clang

              # ── Utilities ─────────────────────────────────────────
              git-lfs
            ]
            ++ pkgs.lib.optionals isLinux tauriLibs
            ++ pkgs.lib.optionals isDarwin (with pkgs; [
              libiconv
              darwin.apple_sdk.frameworks.WebKit
              darwin.apple_sdk.frameworks.AppKit
            ]);

            # uv env + native libs for both Rust (webkit/gtk) and Python wheels.
            env = uvEnv // {
              LD_LIBRARY_PATH = mkLibPath (pyRuntimeLibs
                ++ pkgs.lib.optionals isLinux tauriLibs);
            };

            # Appended after the guardrails banner + hook-install. The prek-install
            # block is gone — mkDevShell does it (worktree-safe). Project bootstrap
            # (venv/npm/submodule) stays, skipped in CI.
            hook = ''
              if [ -z "''${CI:-}" ]; then
                # ── Python venv via uv ──────────────────────────────
                if [ ! -d ".venv" ]; then uv venv; fi
                uv sync --frozen 2>/dev/null || uv sync

                # ── Node deps ───────────────────────────────────────
                if [ ! -d "node_modules" ] && [ -f "frontend/package.json" ]; then
                  (cd frontend && (npm ci --prefer-offline 2>/dev/null || npm install))
                fi

                # ── nucl-parquet submodule ──────────────────────────
                if [ ! -f "nucl-parquet/data/catalog.json" ]; then
                  echo "[hyrr] initializing nucl-parquet submodule..."
                  git submodule update --init nucl-parquet 2>/dev/null || true
                fi

                echo "hyrr devshell — python $(python3 --version 2>&1 | cut -d' ' -f2), rustc $(rustc --version | cut -d' ' -f2), node $(node --version)"
              fi

              # ── Nix clang for cc-rs (Rust builds) ─────────────────
              export CC="${pkgs.clang}/bin/clang"
              export CXX="${pkgs.clang}/bin/clang++"
            '';
          };
        };

        # ══ checks: `nix flake check` == the CI merge gate (#484) ════════
        # Hermetic, cached, local==runner. The cross-OS e2e + desktop matrices
        # stay on GitHub-hosted runners (real browsers / Windows / macOS can't
        # be a nix sandbox) and run rescoped — see Part B in the workflows.
        checks = {
          # ── Rust (crane) ───────────────────────────────────────────────
          # rustfmt + clippy pinned by flake.lock so `nix develop`'s formatter
          # and CI agree — no "different rustfmt version" drift. Both were never
          # gated before #484; the repo was cleaned to pass them in this PR.
          rust-fmt = craneLib.cargoFmt {
            inherit (commonRustArgs) src pname version;
          };

          # Deny level + intentional allows live in core/Cargo.toml's
          # [lints.clippy] (SSoT, so a bare `cargo clippy` matches CI), hence no
          # `-- --deny warnings` here.
          rust-clippy = craneLib.cargoClippy (commonRustArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets --all-features";
          });

          # Lib + integration tests with the real pinned data (HYRR_DATA), run
          # serially (data_fetch lock-contention tests need --test-threads=1)
          # and `--include-ignored` to pick up the projectile-matrix tier-1 set.
          # Subsumes ci.yml's rust-projectile-matrix + data-fetch-integration.
          rust-test = craneLib.cargoTest (commonRustArgs // {
            inherit cargoArtifacts;
            HYRR_DATA = nuclData;
            cargoTestExtraArgs = "-- --include-ignored --test-threads=1";
          });

          # MCP tool surface (feature `mcp` isn't in the default set) — runs the
          # mcp:: lib tests + the two mcp integration tests against real data.
          # Subsumes ci.yml's mcp-tools job. (A single `--features mcp` run, not
          # a `--lib mcp::` filter, since that filter would also be applied to —
          # and exclude — the `--test` integration binaries.)
          rust-test-mcp = craneLib.cargoTest (commonRustArgs // {
            inherit cargoArtifacts;
            HYRR_DATA = nuclData;
            cargoExtraArgs = "--features mcp";
            cargoTestExtraArgs = "-- --test-threads=1";
          });

          # Embedded data store: build-time tar (#274) + full simulation through
          # it. Subsumes ci.yml's embedded-data-store job.
          rust-test-embed = craneLib.cargoTest (commonRustArgs // {
            inherit cargoArtifacts;
            cargoExtraArgs = "--features embed-data";
            cargoTestExtraArgs = "--test embedded_data_store";
          });

          # PyO3 binding type-drift gate (#181). `cargo check` (not build/test)
          # because the pyo3 `extension-module` feature defers libpython linking
          # — but pyo3's build script still needs an interpreter for ABI
          # detection, hence python + PYO3_PYTHON. py path-deps core, so
          # provision core + the nucl-parquet/hyrr.json siblings. Subsumes
          # ci.yml's hyrr-py-check job.
          py-bindings =
            let
              pyArgs = {
                src = craneLib.cleanCargoSource ./py;
                pname = "hyrr-py";
                version = "0.0.0";
                strictDeps = true;
                preConfigure = provisionCoreSibling;
                nativeBuildInputs = with pkgs; [ pkg-config python ];
                PYO3_PYTHON = "${python}/bin/python3.12";
              };
            in
            craneLib.mkCargoDerivation (pyArgs // {
              cargoArtifacts = craneLib.buildDepsOnly pyArgs;
              buildPhaseCargoCommand = "cargo check --release --offline";
              doInstallCargoArtifacts = false;
            });

          # ── WASM compute backend (crane, wasm32 compile) ───────────────
          # The artifact the frontend imports (#251). Compiles hyrr-wasm for
          # wasm32 — the hermetic gate against the #457-class breakage (wasm
          # won't compile / a core change breaks the wasm import), which
          # silently broke the frontend + desktop builds since v0.12.1. The
          # wasm-bindgen JS-gen + wasm-opt steps stay in wasm-pack (deploy /
          # e2e / tauri jobs), which self-manages the bindgen-cli version. wasm
          # uses core with default-features=false (no parquet-store), but
          # core/build.rs still reads the catalog/hyrr.json siblings.
          wasm-build =
            let
              wasmArgs = {
                src = craneLib.cleanCargoSource ./wasm;
                pname = "hyrr-wasm";
                version = "0.0.0";
                strictDeps = true;
                doCheck = false;
                preConfigure = provisionCoreSibling;
                cargoExtraArgs = "--lib";
                CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
                # wasm32 links with lld (`wasm-ld`); nixpkgs rustc doesn't ship
                # it self-contained, so put it on PATH.
                nativeBuildInputs = with pkgs; [ pkg-config lld ];
              };
            in
            craneLib.buildPackage (wasmArgs // {
              cargoArtifacts = craneLib.buildDepsOnly wasmArgs;
            });

          # ── Pure-source guards (lifted from ci.yml) ────────────────────
          data-fetch-ssot = mkGuard "data-fetch-ssot" ''
            if grep -RnE 'nucl-parquet-data-[0-9]{4}\.[0-9]+\.[0-9]+\.tar\.zst' docs/; then
              echo "::error::docs hardcodes a concrete data-tarball version. Use <V> placeholder; SSoT is hyrr_core::data_fetch::release_url()."
              exit 1
            fi
            if grep -RnE 'nucl-parquet-data-v[0-9]+\.[0-9]+\.[0-9]+\.tar\.zst' docs/; then
              echo "::error::docs uses the legacy v-prefixed tarball name (pre nucl-parquet#151)."
              exit 1
            fi
            grep -q '"https://github.com/exoma-ch/nucl-parquet/releases/download"' core/src/data_fetch.rs || {
              echo "::error::RELEASE_BASE literal in core/src/data_fetch.rs has drifted."
              exit 1
            }
          '';

          release-workflow-guards = mkGuard "release-workflow-guards" ''
            CONF=desktop/src-tauri/tauri.conf.json
            if grep -qE '"before(Build|Dev)Command".*--prefix \.\./frontend"' "$CONF"; then
              echo "::error::beforeBuildCommand/beforeDevCommand uses --prefix ../frontend (needs ../../frontend under Tauri v2)."
              exit 1
            fi
            # Workflows that build the app WITH nuclear data. Post ETH-deploy
            # migration: deploy-eth.yml is the gated data-bearing prod deploy;
            # deploy-frontend.yml ships only the public landing page (no data);
            # promote-to-prod.yml is archived. Mirrors the ci.yml guard (#501).
            for wf in tauri-build deploy-eth e2e e2e-tauri; do
              if ! grep -q 'copy-frontend-data.sh' ".github/workflows/''${wf}.yml"; then
                echo "::error::''${wf}.yml doesn't use scripts/copy-frontend-data.sh — data copy will drift."
                exit 1
              fi
            done
            WF=.github/workflows/release-hyrr-mcp.yml
            COUNT=$(grep -c 'submodule update --init' "$WF")
            if [ "$COUNT" -lt 3 ]; then
              echo "::error::release-hyrr-mcp.yml has only $COUNT submodule-init steps — need ≥3."
              exit 1
            fi
          '';
        };
      });
}
