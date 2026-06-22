{
  description = "hyrr — Hierarchical Yield and Radionuclide Rates";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    # Shared code-quality / observability gates (#159). Pinned to the branch that
    # adds `no-raw-trace-fields`; flip to the default branch once gerchowl/guardrails#24
    # merges. `follows` keeps one nixpkgs eval (shared store/cache).
    guardrails.url = "github:gerchowl/guardrails/no-raw-trace-fields";
    guardrails.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, flake-utils, guardrails, ... }:
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
      });
}
