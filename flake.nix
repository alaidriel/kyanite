{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem
    (system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {
        inherit overlays;
        inherit system;
      };
      rust-stable = pkgs.rust-bin.stable.latest.default.override {
        extensions = ["rust-src"];
      };
    in {
      devShells.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          rust-stable
          llvmPackages_15.libllvm
          # LLVM dependencies
          ncurses # -ltinfo
          libffi # -lffi
          libxml2 # -lxml2
          # Misc
          mdbook
        ];

        LLVM_SYS_150_PREFIX = pkgs.llvmPackages_15.libllvm.dev;
      };
    });
}
