{
	inputs = {
		nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
	};

	outputs = { nixpkgs, ... }:
	let
		pkgs = import nixpkgs { system = "x86_64-linux"; };
	in {
		devShells.x86_64-linux.default = pkgs.mkShell {
			packages = [
				# Python + tooling
				pkgs.python314
				pkgs.uv

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
				pkgs.xorg.libX11
				pkgs.xorg.libXcursor
				pkgs.xorg.libXrandr
				pkgs.xorg.libXinerama
				pkgs.xorg.libXi
				pkgs.xorg.libXxf86vm
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
