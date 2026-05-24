{ pkgs, ... }:
{
  languages.rust = {
    enable = true;
    channel = "stable";
    targets = [ "x86_64-unknown-linux-musl" ];
  };

  git-hooks.hooks = {
    nixfmt.enable = true;
    rustfmt = {
      enable = true;
      entry = "rustfmt";
      pass_filenames = true;
    };
  };

  enterShell = ''
    cat <<EOF
    ─── orma ────────────────────────────────────────────────────────
      cd crates/orma       — enter the Rust crate
      cargo build/test/fmt — once inside the crate
      nix-build nix/       — static-musl release build (from here)
    ─────────────────────────────────────────────────────────────────
    EOF
  '';
}
