{
  description = "pipeflow";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };

        runtimeLibs = with pkgs; [
          pipewire
          wayland
          libxkbcommon
          libGL
          libx11
          libxcursor
          libxi
          libxrandr
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
          protobuf
          makeWrapper
        ];

        runtimeLibraryPath = pkgs.lib.makeLibraryPath runtimeLibs;

        pipeflow = pkgs.rustPlatform.buildRustPackage {
          pname = "pipeflow";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit nativeBuildInputs;
          buildInputs = runtimeLibs;

          postFixup = ''
            wrapProgram $out/bin/pipeflow \
              --prefix LD_LIBRARY_PATH : ${runtimeLibraryPath}
          '';

          meta.mainProgram = "pipeflow";
        };
      in {
        packages.default = pipeflow;

        apps.default = {
          type = "app";
          program = "${pipeflow}/bin/pipeflow";
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
          ] ++ nativeBuildInputs ++ runtimeLibs;

          LD_LIBRARY_PATH = runtimeLibraryPath;
        };
      });
}
