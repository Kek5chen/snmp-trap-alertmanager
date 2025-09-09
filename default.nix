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

  cargoLock.lockFile = ./Cargo.lock;

  doCheck = true;
}
