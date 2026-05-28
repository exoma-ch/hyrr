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
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            # ── Python ──────────────────────────────────────────────
            uv
            ruff

            # ── Rust ────────────────────────────────────────────────
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer

            # ── WASM ────────────────────────────────────────────────
            wasm-pack
            wasm-bindgen-cli

            # ── Node / frontend ─────────────────────────────────────
            nodejs_22

            # ── Build tools ─────────────────────────────────────────
            pkg-config
            clang
            just

            # ── Utilities ───────────────────────────────────────────
            git
            git-lfs
          ]
          # Tauri 2 Linux system deps
          ++ lib.optionals isLinux [
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
          ]
          ++ lib.optionals isDarwin [
            libiconv
            darwin.apple_sdk.frameworks.WebKit
            darwin.apple_sdk.frameworks.AppKit
          ];

          # Expose native libs for Rust builds on NixOS
          LD_LIBRARY_PATH = pkgs.lib.optionalString isLinux
            (pkgs.lib.makeLibraryPath (with pkgs; [
              webkitgtk_4_1
              gtk3
              glib-networking
              libsoup_3
              openssl
              stdenv.cc.cc.lib
            ]));

          shellHook = ''
            # ── Python venv via uv ────────────────────────────────
            if [ ! -d ".venv" ]; then
              uv venv
            fi
            uv sync --frozen 2>/dev/null || uv sync

            # ── Node deps ─────────────────────────────────────────
            if [ ! -d "node_modules" ]; then
              npm ci --prefer-offline 2>/dev/null || npm install
            fi

            # ── nucl-parquet submodule ────────────────────────────
            if [ ! -f "nucl-parquet/data/catalog.json" ]; then
              echo "[hyrr] initializing nucl-parquet submodule..."
              git submodule update --init nucl-parquet 2>/dev/null || true
            fi

            # ── Data env vars ─────────────────────────────────────
            export HYRR_DATA="''${HYRR_DATA:-$(pwd)/nucl-parquet}"

            # ── Nix clang for cc-rs (Rust builds) ─────────────────
            export CC="${pkgs.clang}/bin/clang"
            export CXX="${pkgs.clang}/bin/clang++"
            if [ -n "$NIX_CC" ] && [ -d "$NIX_CC/bin" ]; then
              export PATH="$NIX_CC/bin:$PATH"
            fi

            echo "hyrr devshell — python $(python3 --version 2>&1 | cut -d' ' -f2), rustc $(rustc --version | cut -d' ' -f2), node $(node --version)"
          '';
        };
      });
}
