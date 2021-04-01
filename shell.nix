
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
	buildInputs = with pkgs; [
		# Build Tools
		cargo
		cmake
		pkg-config
		freetype
		fontconfig
	];
}
