# Orma

Loads a system's passwords and machine-id at boot from a volume kept apart
from the system image.

The usual way of getting secrets onto a machine is to put them in the system
statefully (e.g. inside `/var`), seal them to its TPM, or fetch them at first
boot from a service that has to be reachable.

Orma turns that around. The system image holds no secrets, only a schema whose
*fields* describe the passwords, customisations or other types of secrets it
requires. During early boot, the values are read from the *identity volume*
and validated using the schema. The identity volume is typically a separate
disk, TPM-sealed storage or embedded flash, but it can be a simple directory.
Orma does not care how the identity volume is unlocked or mounted, only that
it ends up as a directory it can read.

Orma is not a secrets manager: it neither encrypts the volume nor unlocks it,
and keeps custody of nothing. It starts with a volume already available,
verifies what is on it, and copies the values into place.

The identity volume holds the information distinguishing the specific machine.
Where the medium allows it, a faulty machine is replaced by moving the volume
into the new one; where it does not, as with sealed or soldered storage, a new
volume can be generated from the schema and the machine takes a new identity.

> The name is Romansh for *soul* (Latin *anima* -> *orma*):
> the system "base" image is the vessel and the volume with keys is the soul.

## Schema

A versioned YAML file declaring the fields the system expects.
In principle, the image to be booted contains the schema it requires to boot.
Example:

```yaml
version: 1
fields:
  - path: /machine-id
    type: machine-id
  - path: /user.passwd
    type: hashed-password
    description: Device user password, also accepted at the initramfs unlock prompt
  - path: /sudo.passwd
    type: hashed-password
    description: Administrative password; sudo falls back to the user password when absent
    optional: true
```

The `path` locates the value inside the identity volume: `/machine-id` above
means the file `machine-id` at the volume's top level, not `/machine-id` on the
running system. The *resolve* operation writes it at the same place under the
output directory.

Marking a field `optional` tells orma not to produce it. Resolve accepts a
volume without it, and verifies it like any other when it is there.

See examples in `crate/fixtures/`.

## Operations

Both operations use a *schema* and an *identity volume*.

- **resolve** writes the resolved values into the specified directory. Every
  field is validated, and nothing is written unless all of them are accepted.
  With `--evaluate-only` it reports any invalid or missing values; this can be
  used to test whether an existing identity volume satisfies the schema of an
  updated system image.

- **generate** writes new values into the identity volume.
  This is typically used to populate a new identity volume and to generate any
  missing values on an existing identity volume using `--upgrade` (e.g. when
  the schema has grown a field); with `--dry-run`, the command writes nothing.
  Some field types prompt the operator (e.g. `hashed-password` asks for one).
  Prompts may be answered via the TTY (default), using `systemd-ask-password`,
  or read from stdin (e.g. for use in CI scripts).

  When `--ask-via stdin` and `--dry-run` are used, lines are read from stdin
  to verify all prompts are answered.

Run `orma --help` for more details.

## At boot

Orma runs from the initrd, after the identity volume is mounted and before the
root filesystem is handed over to stage 2:

```ini
[Unit]
Description=Resolve identity volume into the rootfs
Requires=var-lib-identity.mount
After=var-lib-identity.mount sysroot-var.mount
Before=initrd-fs.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/bin/orma resolve /etc/orma/schema.yaml /var/lib/identity /sysroot/var/lib/orma

[Install]
RequiredBy=initrd-fs.target
```

The unit is required by `initrd-fs.target`, so a volume that fails its schema
stops the boot rather than degrading it. A system that reaches stage 2 without
its identity has a machine-id systemd invented on the spot and no password
anybody knows, which is worse than not booting:

```
[  OK  ] Mounted /sysroot/var/lib/identity.
         Starting Resolve identity volume into the rootfs...
[FAILED] Failed to start Resolve identity volume into the rootfs.
See 'systemctl status orma-resolve.service' for details.
[DEPEND] Dependency failed for Initrd File Systems.
[  OK  ] Started Emergency Shell.
[  OK  ] Reached target Emergency Mode.
```

`systemctl status orma-resolve.service` names every field that is missing or
malformed, not just the first. A machine booting for the first time has a blank
volume, so the operator fills it from that shell and carries on:

```
bash-5.3# orma generate /etc/orma/schema.yaml /var/lib/identity
Device user password, also accepted at the initramfs unlock prompt
Passphrase for /user.passwd:
Confirm:
bash-5.3# exit
```

Only `/user.passwd` is asked about: `/machine-id` is made from kernel randomness
rather than from anything the operator knows, and `/sudo.passwd` is optional, so
orma leaves it to the operator.

What orma leaves behind are files. `/var/lib/orma/user.passwd` becomes a login
because the account is configured to read its hash from that path, and
`/var/lib/orma/machine-id` becomes the machine's identity because
`/etc/machine-id` is populated from it. Orma writes the values where the schema
says and stops there; the schema ships with the image, and so does whatever
reads them. The permissions of the written file are decided by the field type.

Those files are not the identity, only a copy of it. Resolve rewrites them from
the volume on every boot, so losing them costs a reboot, and the volume remains
the only place the identity lives.

## Trust model

Encrypting the identity volume, sealing it, deciding who may unlock it and
when: all of that happens before orma runs, and is out of scope. Orma neither
encrypts, holds nor unlocks anything.

The schema turns a machine's identity from an assumption into a statement that
can be checked. Orma checks it as a whole before anything comes to depend on it,
and populates a volume that satisfies it in the first place.

## Limitations

Upgrading refuses an identity volume with an invalid value.

The only mechanism to test needed tools are available at runtime is `--dry-run`.

Two field types exist, `machine-id` and `hashed-password`. Host keys and other
key material are not covered yet. More types will be added as needs land.

Orma does not produce optional fields at all, not even when upgrading, so their
values need to be written by hand. Changing a value orma produces means deleting
it from the volume and upgrading, since upgrading leaves valid values alone.

Nothing ties a volume to a machine. That is what makes moving one to replacement
hardware work, and it means a volume restored onto a second machine gives both
the same identity, which orma will resolve without complaint.

Nothing keeps concurrent runs either: orma takes no lock on the volume.

Directories orma creates may only be entered by their owner, whatever the fields
underneath them are. A `machine-id` declared at `/sub/machine-id` is written
world readable and stays out of reach of the processes meant to read it.

A volume is the only copy of what it holds. Generating a new one gives the
machine a new identity rather than restoring its old one, so anything issued
against the old identity, has to be reissued.
The operator should carefully consider whether they require identity backups.

## Development

This repository uses [devenv](https://devenv.sh/). Enter a development shell
with `devenv shell`. The usual development tasks are documented in the shell
welcome message. See the CI file for the release pipeline.

The latest CI build from `master` can be [downloaded here][release].

[release]: https://gitlab.com/roosemberth/orma/-/jobs/artifacts/master/raw/orma/bin/orma?job=orma
