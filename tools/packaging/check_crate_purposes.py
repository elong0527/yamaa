"""Require a stated purpose for every crate that a host's source archive ships.

A host's source archive ships the crates of its trimmed lock, which
tools/staging/engine_bundle.py stages (#3030 D55): the engine crates, that
host's binding crate, the vendored YAML parser and every crates.io crate that
they reach on any platform with any feature, all vendored. The archive ships
and builds the crates that only build scripts and procedural macros use, and
vendors those that Cargo locks for platforms or features that no yamaa build
selects, so each of them needs a purpose too.

tools/packaging/crate_purposes.toml states why yamaa ships each crate, one
purpose per crate name. This fails while a crate of either host's trimmed lock
has no purpose, a purpose names a crate that neither host ships, or a purpose
is empty.
"""

import sys
import tempfile
import tomllib
from pathlib import Path

from tools.paths import WORKSPACE
from tools.staging import engine_bundle

PURPOSES = Path(__file__).with_name("crate_purposes.toml")
LABEL = "tools/packaging/crate_purposes.toml"
HINT = (
    f"Say in {LABEL} why yamaa ships each new crate, and remove the purposes of "
    "crates that no host ships any more. From rust/, `cargo tree --locked "
    "--offline --workspace --target all -i <crate>` shows what pulls a crate in."
)


def read(path=PURPOSES):
    """The purposes by crate name, from the file's one [crates] table."""
    document = tomllib.loads(path.read_text(encoding="utf-8"))
    crates = document.get("crates")
    if set(document) != {"crates"} or not isinstance(crates, dict):
        raise ValueError(f"{LABEL}: expected only a [crates] table of purposes")
    return crates


def shipped(workspace=WORKSPACE):
    """The crate names of each host's trimmed lock, members included, by host."""
    with tempfile.TemporaryDirectory(prefix="yamaa-crate-purposes-") as scratch:
        return {
            host: {
                name
                for name, _, _ in engine_bundle.stage_workspace(
                    host, Path(scratch) / host, workspace
                )
            }
            for host in sorted(engine_bundle.BINDINGS)
        }


def problems(purposes, graphs):
    """Messages for the crates without a purpose, the purposes of crates that
    no host ships, and the empty purposes.

    `purposes` maps crate names to purposes, and `graphs` maps each host to
    the crate names of its trimmed lock.
    """
    hosts = {}
    for host, names in sorted(graphs.items()):
        for name in names:
            hosts.setdefault(name, []).append(host)
    messages = []
    for name in sorted(set(hosts) - set(purposes)):
        shipping = hosts[name]
        bundles = "bundles ship" if len(shipping) > 1 else "bundle ships"
        messages.append(
            f"{LABEL}: no purpose for {name}, which the "
            f"{' and '.join(shipping)} {bundles}"
        )
    for name in sorted(set(purposes) - set(hosts)):
        messages.append(f"{LABEL}: {name} has a purpose, but no host's bundle ships it")
    for name, purpose in sorted(purposes.items()):
        if not isinstance(purpose, str):
            messages.append(f"{LABEL}: the purpose of {name} must be a string")
        elif not purpose.strip():
            messages.append(f"{LABEL}: the purpose of {name} is empty")
    return messages


def main():
    try:
        purposes = read()
    except ValueError as error:
        raise SystemExit(str(error)) from error
    graphs = shipped()
    messages = problems(purposes, graphs)
    for message in messages:
        print(message, file=sys.stderr)
    if messages:
        print(HINT, file=sys.stderr)
        raise SystemExit(1)
    names = set().union(*graphs.values())
    print(
        f"{LABEL}: a purpose for each of the {len(names)} crates that the "
        f"{' and '.join(sorted(graphs))} bundles ship"
    )


if __name__ == "__main__":
    main()
