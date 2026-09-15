{
  description = "Bims — a 2D top-down game in Rust, running in the browser via WebAssembly";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      eachSystem = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # Build inputs only. `target/` and the checked-in `web/bims.wasm` are
      # outputs, so leaving them out keeps the hash from churning on rebuilds.
      src = nixpkgs.lib.cleanSourceWith {
        src = ./.;
        name = "bims-source";
        filter =
          path: type:
          let
            base = baseNameOf (toString path);
          in
          !(type == "directory" && base == "target") && base != "bims.wasm";
      };

      # Everything the flake exposes, built once per system.
      bimsFor =
        pkgs:
        let
          # The game has no crate dependencies at all, so there is nothing to
          # vendor and `--offline` is enough to build inside the sandbox.
          bims = pkgs.stdenv.mkDerivation {
            pname = "bims";
            version = "0.1.0";
            inherit src;

            nativeBuildInputs = [
              pkgs.rustc # ships the wasm32-unknown-unknown standard library
              pkgs.cargo
              pkgs.lld # nixpkgs' rustc links wasm through the system lld
            ];

            buildPhase = ''
              runHook preBuild
              export CARGO_HOME="$NIX_BUILD_TOP/cargo"
              cargo build --release --locked --offline --target wasm32-unknown-unknown
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              mkdir -p "$out/share/bims"
              cp web/index.html web/bims.js "$out/share/bims/"
              cp target/wasm32-unknown-unknown/release/bims.wasm "$out/share/bims/"
              runHook postInstall
            '';

            meta = {
              description = "A 2D top-down game that runs in the browser";
              platforms = nixpkgs.lib.platforms.all;
            };
          };

          # `nix run` should just put the game in front of you. The page fetches
          # bims.wasm, and fetch is blocked on file:// URLs, so it needs a server.
          bims-serve = pkgs.writeShellApplication {
            name = "bims-serve";
            runtimeInputs = [ pkgs.python3 ];
            text = ''
              exec python3 ${./dev-server.py} --directory ${bims}/share/bims "$@"
            '';
          };
        in
        {
          inherit bims bims-serve;
        };
    in
    {
      packages = eachSystem (
        pkgs:
        let
          built = bimsFor pkgs;
        in
        {
          inherit (built) bims bims-serve;
          default = built.bims;
        }
      );

      apps = eachSystem (
        pkgs:
        let
          serve = {
            type = "app";
            program = nixpkgs.lib.getExe (bimsFor pkgs).bims-serve;
            meta.description = "Serve Bims on http://localhost:8080 (pass a port to change it)";
          };
        in
        {
          inherit serve;
          default = serve;
        }
      );

      # Shared with the plain `nix-shell` entry point, so there is one list of
      # development tools rather than two that drift apart.
      devShells = eachSystem (pkgs: {
        default = import ./shell.nix { inherit pkgs; };
      });

      checks = eachSystem (
        pkgs:
        let
          built = bimsFor pkgs;
        in
        {
          build = built.bims;

          formatting =
            pkgs.runCommand "bims-check-formatting"
              {
                nativeBuildInputs = [
                  pkgs.cargo
                  pkgs.rustfmt
                ];
              }
              ''
                # rustfmt wants a writable tree even when only checking.
                cp -r ${src} source
                chmod -R u+w source
                cd source
                export CARGO_HOME="$NIX_BUILD_TOP/cargo"
                cargo fmt --check
                touch "$out"
              '';
        }
      );

      formatter = eachSystem (pkgs: pkgs.nixfmt-rfc-style);
    };
}
