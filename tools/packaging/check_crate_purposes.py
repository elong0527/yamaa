"""Require a stated purpose for every crate that a host's source archive ships,
and a stated reason for every unconditional engine dependency that #3030
scopes.

A host's source archive ships the crates of its trimmed lock, which
tools/staging/engine_bundle.py stages (#3030 D55): the engine crates, that
host's binding crate, the vendored YAML parser and every crates.io crate that
they reach on any platform with any feature, all vendored. The archive ships
and builds the crates that only build scripts and procedural macros use, and
vendors those that Cargo locks for platforms or features that no yamaa build
selects, so each of them needs a purpose too. The command line's trimmed lock,
with yamaa-cli in place of a binding crate, is the graph that its binaries
build from, and its component inventory lists it.

tools/packaging/crate_purposes.toml states why yamaa ships each crate, one
purpose per crate name. This fails while a crate of any host's trimmed lock
has no purpose, a purpose names a crate that no host ships, or a purpose is
empty.

#3030 section 2.9 scopes some crates to the features and adapters that need
them: Arrow leaves the core and the always-on adapters, staying for the
Python hand-off, Parquet and the Rust facade; serde_json and toml go to the
adapters that use them; and flatbuffers, the encoding of Arrow IPC messages,
goes with arrow-ipc (#3106). Every host builds the engine crates, so a normal
dependency that an engine crate's manifest declares without `optional = true`
is in every host's graph whatever features the host selects. The file's
[unconditional.<engine crate>] tables state why each engine crate depends on
a scoped crate that way. This also fails while such a dependency has no
reason, a reason names a crate that its engine crate does not depend on
unconditionally, or a reason is empty, so the change that moves a crate
behind a feature removes its reason.
"""

import json
import subprocess
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
REASONS_HINT = (
    f"Say in {LABEL}, under [unconditional.<engine crate>], why the engine crate "
    "depends unconditionally on each crate that #3030 section 2.9 scopes, citing "
    "the code that needs it whatever the features, or move the dependency behind "
    "the feature that needs it; remove a reason once its dependency is optional "
    "or gone."
)
# The crates besides Arrow's that #3030 section 2.9 scopes (scoped()).
SCOPED = frozenset({"flatbuffers", "serde_json", "toml"})


def load(path=PURPOSES):
    """The file's [crates] table of purposes and its [unconditional] tables of
    reasons, which may be missing."""
    document = tomllib.loads(path.read_text(encoding="utf-8"))
    crates = document.get("crates")
    reasons = document.get("unconditional", {})
    if (
        not set(document) <= {"crates", "unconditional"}
        or not isinstance(crates, dict)
        or not isinstance(reasons, dict)
    ):
        raise ValueError(
            f"{LABEL}: expected only a [crates] table of purposes and "
            "[unconditional.<engine crate>] tables of reasons"
        )
    return crates, reasons


def read(path=PURPOSES):
    """The purposes by crate name, from the file's [crates] table."""
    return load(path)[0]


def reasons(path=PURPOSES):
    """The reasons by engine crate, then by crate name, from the file's
    [unconditional.<engine crate>] tables."""
    return load(path)[1]


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


def scoped(name):
    """Whether #3030 section 2.9 scopes a crate: an Arrow crate or one of SCOPED."""
    return name == "arrow" or name.startswith("arrow-") or name in SCOPED


def declared(metadata):
    """The scoped crates that each engine crate depends on unconditionally, by
    engine crate.

    `metadata` is the workspace's `cargo metadata --no-deps`. A dependency
    counts when the engine crate's manifest declares it as a normal
    dependency without `optional = true`, on any platform; one that only a
    feature selects, a build or development dependency and a crate that #3030
    does not scope do not.
    """
    packages = {package["name"]: package for package in metadata["packages"]}
    for crate in engine_bundle.ENGINE:
        if crate not in packages:
            raise ValueError(f"rust/Cargo.toml: {crate} is not a workspace member")
    return {
        crate: {
            dependency["name"]
            for dependency in packages[crate]["dependencies"]
            if dependency["kind"] is None
            and not dependency["optional"]
            and scoped(dependency["name"])
        }
        for crate in engine_bundle.ENGINE
    }


def unconditional(workspace=WORKSPACE):
    """declared() for the workspace's manifests."""
    output = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=workspace,
    )
    return declared(json.loads(output))


def listed(names):
    """Name hosts or crates in prose: python and r, or cli, python and r."""
    return " and ".join([", ".join(names[:-1]), names[-1]] if names[1:] else names)


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
            f"{LABEL}: no purpose for {name}, which the {listed(shipping)} {bundles}"
        )
    for name in sorted(set(purposes) - set(hosts)):
        messages.append(f"{LABEL}: {name} has a purpose, but no host's bundle ships it")
    for name, purpose in sorted(purposes.items()):
        if not isinstance(purpose, str):
            messages.append(f"{LABEL}: the purpose of {name} must be a string")
        elif not purpose.strip():
            messages.append(f"{LABEL}: the purpose of {name} is empty")
    return messages


def reason_problems(reasons, dependencies):
    """Messages for the unconditional dependencies without a reason, the
    reasons for dependencies that are not unconditional or not scoped, the
    reasons of crates that are not engine crates, and the empty reasons.

    `reasons` maps engine crates to their reasons by crate name, and
    `dependencies` maps each engine crate to the scoped crates that it
    depends on unconditionally (declared()).
    """
    messages = []
    for crate in sorted(set(reasons) - set(dependencies)):
        messages.append(
            f"{LABEL}: [unconditional.{crate}] names no engine crate; the engine "
            f"crates are {listed(sorted(dependencies))}"
        )
    for crate, names in sorted(dependencies.items()):
        recorded = reasons.get(crate, {})
        if not isinstance(recorded, dict):
            messages.append(
                f"{LABEL}: [unconditional.{crate}] must be a table of reasons"
            )
            continue
        for name in sorted(names - set(recorded)):
            messages.append(
                f"{LABEL}: no reason why {crate} depends on {name} "
                f"unconditionally, although #3030 section 2.9 scopes {name} to "
                "the features or adapters that need it"
            )
        for name in sorted(set(recorded) - names):
            if scoped(name):
                messages.append(
                    f"{LABEL}: a reason why {crate} depends on {name} "
                    "unconditionally, which it no longer does"
                )
            else:
                messages.append(
                    f"{LABEL}: a reason why {crate} depends on {name} "
                    f"unconditionally, but #3030 section 2.9 does not scope {name}"
                )
        for name, reason in sorted(recorded.items()):
            if not isinstance(reason, str):
                messages.append(
                    f"{LABEL}: the reason why {crate} depends on {name} must be "
                    "a string"
                )
            elif not reason.strip():
                messages.append(
                    f"{LABEL}: the reason why {crate} depends on {name} is empty"
                )
    return messages


def main():
    try:
        purposes, stated = read(), reasons()
        dependencies = unconditional()
    except ValueError as error:
        raise SystemExit(str(error)) from error
    graphs = shipped()
    messages = problems(purposes, graphs)
    unexplained = reason_problems(stated, dependencies)
    for message in messages + unexplained:
        print(message, file=sys.stderr)
    if messages:
        print(HINT, file=sys.stderr)
    if unexplained:
        print(REASONS_HINT, file=sys.stderr)
    if messages or unexplained:
        raise SystemExit(1)
    names = set().union(*graphs.values())
    print(
        f"{LABEL}: a purpose for each of the {len(names)} crates that the "
        f"{listed(sorted(graphs))} bundles ship"
    )
    count = sum(map(len, dependencies.values()))
    print(
        f"{LABEL}: a reason for each of the {count} unconditional dependencies of "
        "the engine crates on crates that #3030 section 2.9 scopes"
    )


if __name__ == "__main__":
    main()
