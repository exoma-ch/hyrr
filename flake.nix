{
  description = "hyrr — Hierarchical Yield and Radionuclide Rates";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
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
        # plus prek itself and git. ruff + pyright come from pythonTooling. All
        # from nixpkgs so they run on NixOS (prek's downloaded generic-linux
        # hook binaries hit the stub-ld wall otherwise). Shared by the `ci` and
        # `default` shells so `prek run --all-files` behaves identically in CI
        # and locally — one lint gate, one set of pinned versions.
        hookTooling = with pkgs; [ prek typos typstyle just git ];

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
          # Python toolchain + the prek hook tools — fast to realize (no
          # WebKitGTK / Rust / Node), used by CI lint + test jobs and for local
          # Python-only work. Carries hookTooling so CI's lint job can run the
          # full `prek run --all-files` gate (the same one local dev runs).
          ci = pkgs.mkShell ({
            packages = pythonTooling ++ hookTooling;
            LD_LIBRARY_PATH = mkLibPath pyRuntimeLibs;
          } // uvEnv);

          # ── Full dev shell ───────────────────────────────────────────
          default = pkgs.mkShell ({
            # pythonTooling + hookTooling (prek, typos, typstyle, just, git) are
            # shared with the `ci` shell so the prek gate is identical in CI and
            # locally; the extras below are the full desktop/WASM/frontend stack.
            packages = with pkgs; pythonTooling ++ hookTooling ++ [
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

            # Native libs for both Rust (webkit/gtk) and Python wheels.
            LD_LIBRARY_PATH = mkLibPath (pyRuntimeLibs
              ++ pkgs.lib.optionals isLinux tauriLibs);

            shellHook = ''
              # Skip auto-bootstrap in CI / non-interactive use — `nix develop`
              # there just provides tools; jobs run their own steps explicitly.
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

                # ── Git hooks via prek (reads .pre-commit-config.yaml) ──
                if [ -d .git ] && [ ! -f .git/hooks/pre-commit ]; then
                  prek install 2>/dev/null || true
                fi

                echo "hyrr devshell — python $(python3 --version 2>&1 | cut -d' ' -f2), rustc $(rustc --version | cut -d' ' -f2), node $(node --version)"
              fi

              # ── Nix clang for cc-rs (Rust builds) ─────────────────
              export CC="${pkgs.clang}/bin/clang"
              export CXX="${pkgs.clang}/bin/clang++"
            '';
          } // uvEnv);
        };
      });
}
