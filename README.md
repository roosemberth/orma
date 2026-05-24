# Orma

Loads a system's passwords and keys at boot from a separate volume.

This tool is designed for systems booting images that carry no identity.
The operator makes the volume available (e.g. by unlocking it); orma then reads
its declared fields, validates the record requested by the system as a whole,
and writes the values to a stable path for the rest of the system to consume.
If any field is missing or invalid, orma writes nothing and exits non-zero.

The name is Romansh for *soul* (Latin *anima* -> *orma*): the system “base”
image is the body and the volume with keys is the soul.

## Schema

A versioned YAML file declaring the fields the system expects. Each field
has a path inside the volume, a type, and may be marked optional. In principle,
the image to be booted contains the schema it requires to boot.

See `crates/orma/fixtures/schema-example.yaml` for an example.

## Operations

The following operations work against a volume:

- **Resolve** turns the volume into the values the system consumes. It
  validates every declared field as a unit and lays the values at an
  output path. If any field fails, nothing is written.
- **Generate** can be used to populate the contents of a volume.
  When invoked interactively, it prompts for or generates a value for every
  declared field and writes them into the volume.

Run `orma --help` for command-line details.

## Field types

- **`hashed-password`**: valid if the stored value is a well-formed
  crypt record. Generate prompts for a passphrase and shells out to
  `mkpasswd -m yescrypt -s`.

More types will be added as needs land.

## Trust model

Orma is a correctness mechanism, not a security boundary. Security is provided
by the encryption of the volume itself, whose passphrase is the trust anchor.
Orma runs against an already-unlocked volume, and is unable to detect or prevent
its own code being tampered with, which would defeat any policy Orma might
pretend to enforce.

## Build

The Nix derivation under `nix/default.nix` builds a static-pie musl binary
suitable for an initrd.

For development, enter `devenv shell`.

The latest CI build from `master` can be [downloaded here](https://gitlab.com/roosemberth/orma/-/jobs/artifacts/master/raw/orma/bin/orma?job=build).
