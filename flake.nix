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
				pkgs.zlib
				pkgs.stdenv.cc.cc.lib
				pkgs.libx11
				pkgs.libxcursor
				pkgs.libxext
				pkgs.libxfixes
				pkgs.libxrandr
				pkgs.libxinerama
				pkgs.libxi
				pkgs.libxxf86vm
				pkgs.alsa-lib
				pkgs.vulkan-loader
			]}";
			packages = [
				pkgs.python314
				pkgs.uv
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

				# Build tools (pygame WILL need these)
				pkgs.gcc
				pkgs.pkg-config

				# SDL + deps for pygame
				pkgs.SDL2
				pkgs.SDL2_image
				pkgs.SDL2_mixer
				pkgs.SDL2_ttf
				pkgs.freetype
				pkgs.libpng
				pkgs.zlib
				pkgs.libjpeg

				# Runtime libs pygame probes for
				pkgs.libx11
				pkgs.libxcursor
				pkgs.libxext
				pkgs.libxfixes
				pkgs.libxrandr
				pkgs.libxinerama
				pkgs.libxi
				pkgs.libxxf86vm
				pkgs.alsa-lib
			];

			# pygame expects these to exist
			shellHook = ''
				export SDL_VIDEODRIVER=x11
				export SDL_AUDIODRIVER=alsa
			'';
		};
	};
}
