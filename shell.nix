
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
	shellHook = ''
    	export PGDATA=./db/content
	'';
}
