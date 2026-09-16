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
              # --workspace, not just the root package: `time`, `physics` and
              # `worldgen` are meant to compile for wasm32 as well as for the
              # native server, and nothing else in the build would ever find
              # out if one of them stopped.
              cargo build --release --locked --offline --target wasm32-unknown-unknown --workspace
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              mkdir -p "$out/share/bims"
              # Both front ends out of one directory: the room is index.html,
              # the menus in front of it are builder.html. They share the wasm,
              # and one day the builder will start the game with it.
              cp web/index.html web/bims.js "$out/share/bims/"
              cp web/builder.html web/builder.js "$out/share/bims/"
              cp target/wasm32-unknown-unknown/release/bims.wasm "$out/share/bims/"
              runHook postInstall
            '';

            meta = {
              description = "A 2D top-down game that runs in the browser";
              platforms = nixpkgs.lib.platforms.all;
            };
          };

          # `nix run` should just put the thing in front of you. The pages
          # fetch bims.wasm, and fetch is blocked on file:// URLs, so they
          # need a server.
          #
          # One per front end, differing only in which page is opened and
          # which port it falls back to. Separate ports on purpose: the room
          # and the builder come out of the same directory, so a shared
          # default would have the second one find the first already there and
          # hand you the wrong page.
          serveFor =
            {
              name,
              page,
              port,
              about,
            }:
            pkgs.writeShellApplication {
              inherit name;
              runtimeInputs = [ pkgs.python3 ];
              text = ''
                exec python3 ${./dev-server.py} \
                  --directory ${bims}/share/bims \
                  --page ${page} --default-port ${toString port} "$@"
              '';
              meta.description = about;
            };

          bims-serve = serveFor {
            name = "bims-serve";
            page = "index.html";
            port = 8080;
            about = "Serve the Bims room on http://localhost:8080";
          };

          bims-builder = serveFor {
            name = "bims-builder";
            page = "builder.html";
            port = 8081;
            about = "Serve the Bims builder on http://localhost:8081";
          };
        in
        {
          inherit bims bims-serve bims-builder;
        };
    in
    {
      packages = eachSystem (
        pkgs:
        let
          built = bimsFor pkgs;
        in
        {
          inherit (built) bims bims-serve bims-builder;
          default = built.bims;
        }
      );

      # One app per thing you can run. `nix run .#game` is the room and is the
      # default; `nix run .#builder` is the menus in front of it. More will
      # follow, and each is a name here rather than a flag on one app.
      apps = eachSystem (
        pkgs:
        let
          built = bimsFor pkgs;

          game = {
            type = "app";
            program = nixpkgs.lib.getExe built.bims-serve;
            meta.description = "Play Bims on http://localhost:8080 (pass a port to change it)";
          };

          builder = {
            type = "app";
            program = nixpkgs.lib.getExe built.bims-builder;
            meta.description = "The start menu, setup and lobby on http://localhost:8081";
          };
        in
        {
          inherit game builder;
          # The old name for the game, kept so `nix run .#serve` still works.
          serve = game;
          default = game;
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

          # The parts that are pure arithmetic — travel times, ship mass, the
          # world generator — carry their own unit tests, and those run
          # natively because a wasm test harness would need a runtime to host
          # it. The room itself is still checked by the probes and harnesses
          # in scratchpad/, which want a real terminal.
          tests =
            pkgs.runCommand "bims-check-tests"
              {
                nativeBuildInputs = [
                  pkgs.cargo
                  pkgs.rustc
                  pkgs.stdenv.cc # a native test binary is linked with cc, not lld
                ];
              }
              ''
                cp -r ${src} source
                chmod -R u+w source
                cd source
                export CARGO_HOME="$NIX_BUILD_TOP/cargo"
                # Named packages rather than --workspace: the room is a
                # cdylib meant for wasm and its tests are the probes in
                # scratchpad/, which want a terminal. These three are plain
                # libraries and their tests are plain `cargo test`.
                cargo test --locked --offline -p time -p physics -p worldgen \
                  --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget}
                touch "$out"
              '';

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
