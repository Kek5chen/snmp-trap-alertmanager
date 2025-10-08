{
  rustPlatform,
  perl,
  ...
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = "snmp-trap-alertmanager";
  version = cargoToml.package.version;

  src = ./.;

  nativeBuildInputs = [
    perl
  ];

  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "tera-1.20.0" = "sha256-28TZWznuiijkbrBHZ8ZvWP6+OWQ4nwTkzNSWu4lul1c=";
    };
  };

  doCheck = true;
}
