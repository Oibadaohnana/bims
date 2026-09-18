{
  description = "Bims — a 2D top-down game in Rust and Bevy";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      eachSystem = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # Build inputs only. `target/` is an output, so leaving it out keeps
      # the hash from churning on rebuilds.
      src = nixpkgs.lib.cleanSourceWith {
        src = ./.;
        name = "bims-source";
        filter =
          path: type:
          let
            base = baseNameOf (toString path);
          in
          !(type == "directory" && (base == "target" || base == "result"));
      };

      # Everything the flake exposes, built once per system.
      bimsFor =
        pkgs:
        let
          # What winit and wgpu open at runtime: the window system and the
          # GPU loader. None of it is linked at build time, so the binary is
          # wrapped to find them rather than built against them.
          runtimeLibs = with pkgs; [
            vulkan-loader
            libxkbcommon
            wayland
            libx11
            libxcursor
            libxi
            libxrandr
          ];

          bims = pkgs.rustPlatform.buildRustPackage {
            pname = "bims";
            version = "0.1.0";
            inherit src;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.makeWrapper ];

            # The whole workspace is tested — the rules crates carry the
            # tests that pin the world's arithmetic, and the app the ones
            # that pin its tables against them — but only the app is built
            # for the result, since it is the one binary.
            cargoBuildFlags = [
              "-p"
              "app"
            ];
            cargoTestFlags = [ "--workspace" ];

            postInstall = ''
              wrapProgram "$out/bin/bims" \
                --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
            '';

            meta = {
              description = "A 2D top-down game: a crew, a ship and a system to fly it round";
              mainProgram = "bims";
              platforms = systems;
            };
          };

          # One per thing to run. Each is a name rather than a flag on one
          # app, so `nix run .#room` reads as what it is.
          runFor =
            {
              name,
              what,
              about,
            }:
            pkgs.writeShellApplication {
              inherit name;
              text = ''exec ${nixpkgs.lib.getExe bims} ${what} "$@"'';
              meta.description = about;
            };
        in
        {
          inherit bims;
          bims-game = runFor {
            name = "bims-game";
            what = "game";
            about = "Play Bims — menu, lobby, world, ship, then the game";
          };
          bims-simulation = runFor {
            name = "bims-simulation";
            what = "simulation";
            about = "Straight into the game world on the playtest ship";
          };
          bims-design = runFor {
            name = "bims-design";
            what = "design";
            about = "Straight into the yard, the playtest ship given, docked where the simulation docks";
          };
          bims-room = runFor {
            name = "bims-room";
            what = "room";
            about = "The behaviour test room — Bims on a deck";
          };
          bims-test = runFor {
            name = "bims-test";
            what = "test";
            about = "Docked at a random station in a random galaxy, on the playtest ship";
          };
          bims-combat = runFor {
            name = "bims-combat";
            what = "combat";
            about = "The simulation docked at a hostile station, the people living there enemies";
          };
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
            bims-design
            bims-room
            bims-test
            bims-combat
            ;
          default = built.bims;
        }
      );

      # One app per thing you can run. `nix run .#game` is the whole game in
      # the order a player meets it and is the default; `.#simulation` skips
      # to the world on a prebuilt ship; `.#design` skips to the yard with
      # that ship given; `.#room` is the behaviour test room; `.#test` is the
      # simulation somewhere else each time; `.#combat` is the simulation at
      # a hostile station.
      apps = eachSystem (
        pkgs:
        let
          built = bimsFor pkgs;
          app = drv: about: {
            type = "app";
            program = nixpkgs.lib.getExe drv;
            meta.description = about;
          };
        in
        rec {
          game = app built.bims-game "Play Bims — menu, lobby, world, ship, then the game";
          simulation = app built.bims-simulation "Straight into the game world on the playtest ship";
          design = app built.bims-design "Straight into the yard, the playtest ship given, docked where the simulation docks";
          room = app built.bims-room "The behaviour test room — Bims on a deck";
          test = app built.bims-test "Docked at a random station in a random galaxy, on the playtest ship";
          combat = app built.bims-combat "The simulation docked at a hostile station, the people living there enemies";
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
          # Builds the app and runs every crate's tests: buildRustPackage's
          # check phase is `cargo test`, with the flags above.
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
