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
      entry = ''
        bash -c '
          cd crate
          cargo fmt
        '
      '';
      files = "\\.rs$";
      pass_filenames = false;
    };
    clippy = {
      enable = true;
      entry = ''
        bash -c '
          cd crate
          cargo clippy --all-targets -- --deny warnings
        '
      '';
      files = "\\.rs$";
      pass_filenames = false;
    };
  };

  enterShell = ''
    cat <<EOF
    ─── orma ────────────────────────────────────────────────────────
      cd crate             — enter the Rust crate
      cargo build/test/fmt — once inside the crate
    ─────────────────────────────────────────────────────────────────
    EOF
  '';
}
