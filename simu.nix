{ lib, pkgs, rustPlatform }: rustPlatform.buildRustPackage {
  pname = "simu";
  version = "0.1.0";
  src = builtins.path { path = ./.; name = "simu"; };

  buildInputs = with pkgs; [ pam ];

  cargoLock = {
    lockFile = builtins.path { path = ./Cargo.lock; name = "simu"; };
  };
}

