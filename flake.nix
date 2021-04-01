{
  	inputs = {
		utils.url = "github:numtide/flake-utils";
		naersk.url = "github:nmattia/naersk";
		mozillapkgs = {
			url = "github:mozilla/nixpkgs-mozilla";
			flake = false;
		};
  	};

  	outputs = { self, nixpkgs, utils, naersk, mozillapkgs }:
	utils.lib.eachDefaultSystem (system: let
		pkgs = nixpkgs.legacyPackages."${system}";
		# Get a specific rust version
		mozilla = pkgs.callPackage (mozillapkgs + "/package-set.nix") {};
		rust = (mozilla.rustChannelOf {
			date = "2021-03-31"; # get the current date with `date -I`
			channel = "nightly";
			sha256 = "sha256-oK5ebje09MRn988saJMT3Zze/tRE7u9zTeFPV1CEeLc=";
		}).rust;
		# Override the version used in naersk
		naersk-lib = naersk.lib."${system}".override {
			cargo = rust;
			rustc = rust;
		};
	in rec {
		# `nix build`
		packages.dbr-sim = naersk-lib.buildPackage {
			pname = "dbr-sim";
			root = ./.;
			nativeBuildInputs = with pkgs; [
				cmake
				pkg-config
				fontconfig
			];
			buildInputs = with pkgs; [
				freetype
			];
		};
		defaultPackage = packages.dbr-sim;

		# `nix run`
		apps.dbr-sim = utils.lib.mkApp {
			drv = packages.dbr-sim;
		};
		defaultApp = apps.dbr-sim;

		# `nix develop`
		devShell = pkgs.mkShell {
			nativeBuildInputs = packages.dbr-sim.nativeBuildInputs;
		};
	});
}