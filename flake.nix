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

      # Build inputs only. `target/` and the built wasm in `web/` are outputs,
      # so leaving them out keeps the hash from churning on rebuilds. One
      # entry per cdylib: a new front end's wasm added here is a hash that
      # changes every time anybody runs ./build.sh.
      wasmOutputs = [
        "bims.wasm"
        "ship.wasm"
        "lobby.wasm"
      ];

      src = nixpkgs.lib.cleanSourceWith {
        src = ./.;
        name = "bims-source";
        filter =
          path: type:
          let
            base = baseNameOf (toString path);
          in
          !(type == "directory" && base == "target") && !(builtins.elem base wasmOutputs);
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
              # --workspace, not just one package: `time`, `physics`,
              # `worldgen` and `shipdesign` are meant to compile for wasm32 as
              # well as for the native server, and nothing else in the build
              # would ever find out if one of them stopped. It is also what
              # builds the other two cdylibs — `ship`, the design phase, and
              # `lobby`, the World tab in front of it.
              cargo build --release --locked --offline --target wasm32-unknown-unknown --workspace
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              mkdir -p "$out/share/bims"
              # Every front end out of one directory: the room is index.html,
              # the menus in front of it are builder.html, and the design
              # phase they start is ship.html. They differ only in which page
              # is opened — which is why each has its own default port below,
              # and why the builder can navigate straight to ship.html.
              cp web/index.html web/bims.js "$out/share/bims/"
              cp web/builder.html web/builder.js "$out/share/bims/"
              cp web/ship.html web/ship.js "$out/share/bims/"
              # One wasm per cdylib. A page whose wasm was not copied fetches
              # a 404 and shows nothing at all.
              cp target/wasm32-unknown-unknown/release/bims.wasm "$out/share/bims/"
              cp target/wasm32-unknown-unknown/release/ship.wasm "$out/share/bims/"
              cp target/wasm32-unknown-unknown/release/lobby.wasm "$out/share/bims/"
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
          # One per thing to run, differing in which page is opened, which
          # modules identify its build, and which port it falls back to.
          #
          # Separate ports on purpose: every front end comes out of the same
          # directory, so a shared default would have the second one find the
          # first already there and hand you the wrong page. And every module
          # a front end loads goes into `--wasm`, for the same class of reason
          # — the server tells builds apart by hashing them together, so the
          # game, which is two pages and two modules, would otherwise look
          # unchanged after a rebuild that changed only the designer's.
          serveFor =
            {
              name,
              page,
              wasm,
              port,
              about,
            }:
            pkgs.writeShellApplication {
              inherit name;
              runtimeInputs = [ pkgs.python3 ];
              text = ''
                exec python3 ${./dev-server.py} \
                  --directory ${bims}/share/bims \
                  --page '${page}' ${nixpkgs.lib.concatMapStringsSep " " (w: "--wasm ${w}") wasm} \
                  --default-port ${toString port} "$@"
              '';
              meta.description = about;
            };

          bims-game = serveFor {
            name = "bims-game";
            page = "builder.html";
            wasm = [
              "lobby.wasm"
              "ship.wasm"
            ];
            port = 8080;
            about = "Serve the whole of Bims on http://localhost:8080";
          };

          bims-simulation = serveFor {
            name = "bims-simulation";
            page = "ship.html?mode=1";
            wasm = [ "ship.wasm" ];
            port = 8083;
            about = "Serve the Bims simulation on http://localhost:8083";
          };

          bims-room = serveFor {
            name = "bims-room";
            page = "index.html";
            wasm = [ "bims.wasm" ];
            port = 8084;
            about = "Serve the Bims behaviour test room on http://localhost:8084";
          };
        in
        {
          inherit
            bims
            bims-game
            bims-simulation
            bims-room
            ;
        };
    in
    {
      packages = eachSystem (
        pkgs:
        let
          built = bimsFor pkgs;
        in
        {
          inherit (built)
            bims
            bims-game
            bims-simulation
            bims-room
            ;
          default = built.bims;
        }
      );

      # One app per thing you can run, and each is a name here rather than a
      # flag on one app. `nix run .#game` is the whole game in the order a
      # player meets it and is the default; `.#simulation` skips to the world
      # on a prebuilt ship; `.#room` is the behaviour test room. The old
      # `builder`, `ship` and `serve` names are gone rather than aliased: two
      # names for one port is how the wrong page gets opened.
      apps = eachSystem (
        pkgs:
        let
          built = bimsFor pkgs;

          game = {
            type = "app";
            program = nixpkgs.lib.getExe built.bims-game;
            meta.description = "Play Bims — menu, lobby, world, ship, then the game — on http://localhost:8080";
          };

          simulation = {
            type = "app";
            program = nixpkgs.lib.getExe built.bims-simulation;
            meta.description = "Straight into the game world on the playtest ship, on http://localhost:8083";
          };

          room = {
            type = "app";
            program = nixpkgs.lib.getExe built.bims-room;
            meta.description = "The behaviour test room — Bims on a deck — on http://localhost:8084";
          };
        in
        {
          inherit game simulation room;
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
          # world generator, the rules for laying out a ship — carry their own
          # unit tests, and those run natively because a wasm test harness
          # would need a runtime to host it. The room and the designer are
          # still checked by the probes and harnesses in scratchpad/, which
          # want a real terminal.
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
                # Named packages rather than --workspace: `bims` is a cdylib
                # meant for wasm and its tests are the probes and harnesses in
                # scratchpad/, which want a terminal. The eight libraries have
                # plain `cargo test` tests.
                #
                # `ship` and `lobby` are on the list and are cdylibs, which is
                # the one exception and a narrow one: each is also an rlib, and
                # the geometry that turns a design tile — or a star — into a
                # place on screen and back is pure arithmetic that gets
                # silently wrong in a way no harness can see. Everything else
                # in them is still checked by scratchpad/ship-check.mjs and
                # scratchpad/builder-check.mjs against the real pages.
                cargo test --locked --offline \
                  -p time -p physics -p worldgen -p shipdesign -p economy \
                  -p health -p flight -p world -p ship -p lobby \
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
