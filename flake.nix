{
	inputs = {
		nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
	};

	outputs = { nixpkgs, ... }:
	let
		pkgs = import nixpkgs { system = "x86_64-linux"; };
	in {
		devShells.x86_64-linux.default = pkgs.mkShell {
			LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${pkgs.lib.makeLibraryPath [
				pkgs.vulkan-loader
			]}";
			packages = [
				pkgs.cargo
				pkgs.rustc
				pkgs.rustfmt
				pkgs.clippy
				pkgs.rust-analyzer
				pkgs.wasm-pack
				pkgs.bun
				pkgs.cargo-watch
				pkgs.lld
				pkgs.vulkan-loader
			];
		};
	};
}
