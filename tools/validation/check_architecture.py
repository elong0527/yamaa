"""Check the engine architecture of #3030 section 2.10.

Two rules hold for the Rust workspace under `rust/`:

- **Crate edges.** Core depends on nothing else in the workspace, the
  adapters depend on core, the application layer that orchestrates prepare,
  bind, execute and publish depends on the adapters and core (#3761), the
  facade composes the application, the adapters and core, and the boundary
  that the two host bindings negotiate through depends on nothing else in the
  workspace. The Python and R bindings depend on the facade and
  the boundary only, and the command line on the facade only. Every
  dependency table counts: normal, build and development
  dependencies, on every target, under any rename, including a rename that a
  member inherits from `[workspace.dependencies]`. A vendored crate outside
  the workspace, such as the adapters' YAML parser, is outside the rule, and
  a workspace member that `LAYER` gives no layer fails until it has one.
- **No file or format code in core.** A scan of `yamaa-core`'s sources,
  outside comments and the items that compile only for tests, refuses the
  marks such code leaves. File code is a path into the standard library's
  file, IO, network, process, environment, OS or path modules, or its clock.
  Format code decodes bytes as text, or writes a format's mark in a literal:
  an XML declaration, namespace or escape, a SAS transport header record or
  the Parquet magic number. The scan reads marks, so it is a check, not a
  proof, and it admits no exception.
"""

import argparse
import re
import sys
import tomllib
from pathlib import Path

from tools.paths import ROOT
from tools.validation.check_diagnostics import (
    RUST_PATH,
    RustImports,
    blank,
    cfg_test_items,
    rust_tokens,
)

SELF = "tools/validation/check_architecture.py"
WORKSPACE = Path("rust")
CORE_SOURCES = Path("rust/crates/yamaa-core/src")

# Each workspace crate's layer (#3030 section 2.10).
LAYER = {
    "yamaa-core": "core",
    "yamaa-adapters": "adapters",
    "yamaa-application": "application",
    "yamaa": "facade",
    "yamaa-boundary": "boundary",
    "yamaa-cli": "host",
    "yamaa-python": "binding",
    "yamaa-r": "binding",
}
# The layers that each layer's crates may depend on, and the rule that says so.
ALLOWED = {
    "core": frozenset(),
    "adapters": frozenset({"core"}),
    "application": frozenset({"adapters", "core"}),
    "facade": frozenset({"application", "adapters", "core"}),
    "boundary": frozenset(),
    "binding": frozenset({"facade", "boundary"}),
    "host": frozenset({"facade"}),
}
RULE = {
    "core": "core depends on nothing else in the workspace",
    "adapters": "the adapters depend on core alone",
    "application": "the application layer depends on the adapters and core alone",
    "facade": "the facade depends on the application, the adapters and core alone",
    "boundary": "the boundary depends on nothing else in the workspace",
    "binding": "a host binding depends on the facade and the boundary alone",
    "host": "a host depends on the facade alone",
}
# Cargo still reads the underscore spellings of the last two tables.
DEPENDENCY_TABLES = (
    "dependencies",
    "build-dependencies",
    "build_dependencies",
    "dev-dependencies",
    "dev_dependencies",
)

# The standard library's modules that reach files, processes and the host,
# and its clock.
STD_IO = frozenset({"env", "fs", "io", "net", "os", "path", "process"})
CLOCK = frozenset({"Instant", "SystemTime", "UNIX_EPOCH"})
DECODE = re.compile(r"\bfrom_utf8(?:_lossy(?:_owned)?|_unchecked(?:_mut)?)?\b")
# Marks of a format's bytes in a string literal, each with what it is.
SIGNATURES = (
    ("<?xml", "an XML declaration"),
    ("xmlns", "an XML namespace"),
    ("&amp;", "an XML escape"),
    ("&lt;", "an XML escape"),
    ("&gt;", "an XML escape"),
    ("&quot;", "an XML escape"),
    ("&apos;", "an XML escape"),
    ("HEADER RECORD", "a SAS transport header record"),
    ("PAR1", "the Parquet magic number"),
)
HINT = (
    "#3030 section 2.10 keeps file and format code in the adapters, and core "
    "keeps the metadata models: move the code to the adapters, behind a port."
)


# -- crate edges --------------------------------------------------------------


def members(root):
    """Each workspace member's manifest path and manifest, by package name, and
    the dependencies a member may inherit (`[workspace.dependencies]`)."""
    workspace = tomllib.loads((root / WORKSPACE / "Cargo.toml").read_text("utf-8"))[
        "workspace"
    ]
    inherited = workspace.get("dependencies", {})
    if not isinstance(inherited, dict):
        raise ValueError("[workspace.dependencies] must be a table")
    excluded = {
        (root / WORKSPACE / path).resolve() for path in workspace.get("exclude", [])
    }
    found = {}
    for pattern in workspace["members"]:
        directories = sorted((root / WORKSPACE).glob(pattern))
        if not directories:
            raise ValueError(f"workspace member {pattern} names no directory")
        for directory in directories:
            if directory.resolve() in excluded:
                continue
            path = directory / "Cargo.toml"
            manifest = tomllib.loads(path.read_text("utf-8"))
            found[manifest["package"]["name"]] = (path, manifest)
    return found, inherited


def dependencies(manifest, inherited):
    """Yield (table, package) for every dependency of a manifest: each table,
    on every target, with a renamed dependency's real package. A dependency
    that declares `workspace = true` takes its package from the entry of the
    same name in `inherited`, the workspace's `[workspace.dependencies]`."""
    tables = [(name, manifest.get(name, {})) for name in DEPENDENCY_TABLES]
    for target, entries in manifest.get("target", {}).items():
        tables += [
            (f"target.{target}.{name}", entries.get(name, {}))
            for name in DEPENDENCY_TABLES
        ]
    for table, entries in tables:
        for key, value in entries.items():
            if isinstance(value, dict) and value.get("workspace") is True:
                if key not in inherited:
                    raise LookupError(
                        f"[{table}] {key} inherits no [workspace.dependencies] entry"
                    )
                value = inherited[key]
            package = value.get("package", key) if isinstance(value, dict) else key
            yield table, package


def edge_errors(root):
    """The workspace edges section 2.10 does not allow."""
    try:
        crates, inherited = members(root)
    except (
        OSError,
        UnicodeError,
        ValueError,
        LookupError,
        TypeError,
        AttributeError,
    ) as error:
        return [
            f"{WORKSPACE.as_posix()}/Cargo.toml: cannot read the workspace: {error}"
        ]
    errors = [
        f"{SELF}: LAYER names {name}, which is not a workspace member"
        for name in sorted(set(LAYER) - set(crates))
    ]
    for name, (path, manifest) in sorted(crates.items()):
        label = path.relative_to(root).as_posix()
        layer = LAYER.get(name)
        if layer is None:
            errors.append(
                f"{label}: {name} has no layer of #3030 section 2.10; "
                f"give it one in {SELF}'s LAYER"
            )
            continue
        try:
            edges = list(dependencies(manifest, inherited))
        except (AttributeError, LookupError, TypeError) as error:
            errors.append(f"{label}: cannot read the dependency tables: {error}")
            continue
        for table, package in edges:
            if package in crates and LAYER.get(package) not in ALLOWED[layer]:
                errors.append(
                    f"{label}: [{table}] {name} depends on {package}, but "
                    f"{RULE[layer]} (#3030 section 2.10)"
                )
    return errors


# -- core sources -------------------------------------------------------------


def io_mark(path):
    """The file code a resolved path reaches, or None."""
    path = tuple(part.removeprefix("r#") for part in path)
    if path[:1] == ("",):
        path = path[1:]
    if len(path) > 1 and path[0] == "std" and path[1] in STD_IO:
        return f"std::{path[1]}"
    if len(path) > 2 and path[:2] == ("std", "time") and path[2] in CLOCK:
        return f"std::time::{path[2]}"
    return None


def marks(source, *, crate_root=False):
    """Each mark of file or format code in production code, as sorted
    (line, kind, mark) triples; comments and test-only items hold none."""
    tokens = list(rust_tokens(source))
    code = blank(source, tokens, {"comment", "character", "string"})
    tests = cfg_test_items(code)
    production = blank(
        code, [("test", low, high, 0, None) for low, high in tests], {"test"}
    )

    def line(offset):
        return source.count("\n", 0, offset) + 1

    found = set()
    imports = RustImports(production, crate_root=crate_root)
    for path, _, offset, scope in imports.leaves:
        for target in imports.resolve(path, scope):
            if isinstance(target, tuple) and (mark := io_mark(target)):
                found.add((line(offset), "file", mark))
    for match in RUST_PATH.finditer(imports.code_without_uses):
        path = tuple(re.split(r"\s*::\s*", match[0]))
        for target in imports.resolve(path, imports.scope_at(match.start())):
            if isinstance(target, tuple) and (mark := io_mark(target)):
                found.add((line(match.start()), "file", mark))
    for match in DECODE.finditer(production):
        found.add((line(match.start()), "format", f"{match[0]} decodes bytes as text"))
    for kind, start, _, first, text in tokens:
        if kind != "string" or any(low <= start < high for low, high in tests):
            continue
        for signature, meaning in SIGNATURES:
            if signature in text:
                found.add((first, "format", f"{signature!r}, {meaning}"))
    return sorted(found)


def core_errors(root):
    """The file and format code in core."""
    directory = root / CORE_SOURCES
    sources = sorted(directory.rglob("*.rs"))
    if not sources:
        return [f"{CORE_SOURCES.as_posix()}: no core sources found"]
    errors = []
    for path in sources:
        relative = path.relative_to(root).as_posix()
        found = marks(
            path.read_text(encoding="utf-8"), crate_root=path == directory / "lib.rs"
        )
        errors.extend(
            f"{relative}:{line}: {kind} code in core: {mark}"
            for line, kind, mark in found
        )
    return errors


def check(root):
    """Every architecture error under the repository `root`."""
    root = Path(root)
    return [*edge_errors(root), *core_errors(root)]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    errors = check(parser.parse_args(argv).root)
    for error in errors:
        print(f"ERROR: {error}")
    if errors:
        print(HINT)
        return 1
    print(
        "PASS: the workspace keeps the crate edges of #3030 section 2.10, and core "
        "holds no file or format code."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
