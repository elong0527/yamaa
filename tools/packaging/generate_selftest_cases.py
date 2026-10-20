"""Generate the facade's table of the selftest cases (#3030 D28, D105, D108).

`benchmarks/selftest.yaml` names the selftest cases: existing benchmarks,
whose expected bytes stay independently authored in `benchmarks/`. The facade
embeds each case's files when it builds, so every host and the command line
run the same bytes through the memory route with no file access. This writes
the table it embeds them from, `rust/crates/yamaa/src/selftest/cases.rs`:
each case's name, its entries and declared submission (from its
benchmark.yaml), the optional features and the adapters it exercises (by
`tools/validation/check_selftest_cases.py`'s evidence), and each file it
embeds, named by its path in the case. The table names files and copies no
bytes.

The crate's build script points the table at the files. Every source archive
carries them in the facade crate's own `selftest/` directory, which
`tools/staging/engine_bundle.py` stages from `case_files`, so a build from an
archive reads nothing outside its package; a build in the repository, where no
such copy exists, reads them from `benchmarks/`, their one source.

The default mode compares the committed table with `benchmarks/`, and
`--write` regenerates it.
"""

import argparse
from pathlib import Path

from tools.benchmark import entries, load_metadata
from tools.paths import ROOT
from tools.validation import check_selftest_cases as listed

TARGET = ROOT / "rust/crates/yamaa/src/selftest/cases.rs"
# The facade crate's directory that holds the staged cases in a source archive.
STAGED = "selftest"
# A case's files that the table does not embed: prose, and the benchmark.yaml
# whose entries and submission the table carries.
LEFT_OUT = ("README.md", "benchmark.yaml")
# The adapters that are optional features of the facade, named as `info()`
# reports them. Every case also needs `yaml`, which reads its specification.
FEATURES = ("odm", "parquet", "readstat")


def names(root=ROOT):
    """The case names that the selftest list under `root` gives, in order.

    Staging reads the list as it stands; `tools.validation.check_selftest_cases`
    checks it, and `load` with it.
    """
    document = listed.load(root / listed.LIST)
    cases = document.get("cases") if isinstance(document, dict) else None
    if not (isinstance(cases, list) and all(isinstance(name, str) for name in cases)):
        raise ValueError(f"{listed.LIST}: expected a list of case names")
    for name in cases:
        if not listed.NAME.fullmatch(name):
            raise ValueError(f"{listed.LIST}: invalid case name {name!r}")
    return cases


def load(root=ROOT):
    """The checked selftest list under `root`: its cases and uncovered adapters."""
    errors = listed.check(root)
    if errors:
        raise ValueError("\n".join(errors))
    document = listed.load(root / listed.LIST)
    return document["cases"], sorted(document.get("uncovered") or {})


def embedded(case: Path):
    """A case's embedded files, as sorted case-relative POSIX paths: what the
    conformance runners stage, which leave out the LEFT_OUT names at any depth,
    and its expected files, each directly in `expected/`."""
    if case.is_symlink() or not case.is_dir():
        raise ValueError(f"{case.name}: expected a benchmark directory")
    paths = []
    for path in sorted(case.rglob("*")):
        relative = path.relative_to(case).as_posix()
        parts = relative.split("/")
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError(f"{case.name}/{relative}: expected a regular file")
        if not path.is_file() or set(parts) & set(LEFT_OUT):
            continue
        if not (relative.isascii() and relative.isprintable()) or any(
            character in relative for character in '"\\'
        ):
            raise ValueError(f"{case.name}/{relative}: unsupported file name")
        if "expected" in parts[1:-1] or (parts[0] == "expected" and len(parts) > 2):
            raise ValueError(
                f"{case.name}/{relative}: an expected file sits directly in expected/"
            )
        paths.append(relative)
    return paths


def case_files(root=ROOT):
    """Yield (path below the staged directory, checkout file) for each file
    that the facade embeds, case by case in list order."""
    for name in names(root):
        case = root / "benchmarks" / name
        for relative in embedded(case):
            yield f"{name}/{relative}", case / relative


def cases(root=ROOT):
    """Each listed case as the table writes it, in list order."""
    result = []
    for name in load(root)[0]:
        case = root / "benchmarks" / name
        metadata = load_metadata(case)
        calls = entries(case, metadata)
        if calls["python"] != calls["r"]:
            raise ValueError(
                f"benchmarks/{name}: a selftest case runs the same entries in "
                "every host"
            )
        adapters = sorted(listed.adapters(case))
        files = embedded(case)
        result.append(
            {
                "name": name,
                "features": sorted({"yaml", *(set(adapters) & set(FEATURES))}),
                "adapters": adapters,
                "entries": [
                    (call["specification"], call["environment"]) for call in calls["r"]
                ],
                "define": metadata.get("define"),
                "files": [path for path in files if not path.startswith("expected/")],
                "expected": [path for path in files if path.startswith("expected/")],
            }
        )
    return result


def literal(text: str) -> str:
    """A Rust string literal for printable ASCII text."""
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def strings(values) -> str:
    """A Rust slice of string literals."""
    return "&[" + ", ".join(literal(value) for value in values) + "]"


def render(root=ROOT) -> str:
    lines = [
        "// Generated by tools/packaging/generate_selftest_cases.py --write. Do not edit.",
        "// Authoritative source: benchmarks/selftest.yaml and the benchmarks it names.",
        "//! The selftest cases in list order, with the files each embeds, and the",
        "//! adapters that no case covers. The table names files; the bytes stay in",
        "//! benchmarks/. The crate's build script sets `YAMAA_SELFTEST_CASES` to the",
        "//! directory that holds them, relative to this file: the copy that every",
        "//! source archive stages in the crate's `selftest/` directory, or",
        "//! benchmarks/ itself in the repository.",
        "use super::{Case, Define, Entry};",
        "",
        "/// One embedded file: its bytes, read when the crate builds.",
        "macro_rules! case_file {",
        "    ($path:literal) => {",
        '        include_bytes!(concat!(env!("YAMAA_SELFTEST_CASES"), "/", $path))',
        "    };",
        "}",
        "",
        "#[rustfmt::skip]",
        "pub(super) const CASES: &[Case] = &[",
    ]
    for case in cases(root):
        name = case["name"]

        def files(paths, strip=""):
            return [
                f"            ({literal(path.removeprefix(strip))}, "
                f"case_file!({literal(f'{name}/{path}')})),"
                for path in paths
            ]

        calls = ", ".join(
            "Entry { specification: "
            + literal(specification)
            + ", project: "
            + ("None" if project is None else f"Some({literal(project)})")
            + " }"
            for specification, project in case["entries"]
        )
        define = case["define"]
        lines += [
            "    Case {",
            f"        name: {literal(name)},",
            f"        features: {strings(case['features'])},",
            f"        adapters: {strings(case['adapters'])},",
            f"        entries: &[{calls}],",
            "        define: "
            + (
                "None,"
                if define is None
                else "Some(Define { project: "
                + literal(define["environment"])
                + ", section: "
                + literal(define["standard"])
                + ", created: "
                + literal(define["created"])
                + " }),"
            ),
            "        files: &[",
            *files(case["files"]),
            "        ],",
            "        expected: &[",
            *files(case["expected"], "expected/"),
            "        ],",
            "    },",
        ]
    _, uncovered = load(root)
    lines += [
        "];",
        "",
        "/// The adapters that no listed case covers (benchmarks/selftest.yaml).",
        "#[rustfmt::skip]",
        f"pub(super) const UNCOVERED: &[&str] = {strings(uncovered)};",
        "",
    ]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    try:
        expected = render()
    except ValueError as error:
        raise SystemExit(str(error)) from error
    if args.write:
        TARGET.parent.mkdir(exist_ok=True)
        TARGET.write_text(expected, encoding="ascii")
    else:
        assert TARGET.is_file() and TARGET.read_text(encoding="ascii") == expected, (
            "the facade's selftest table differs from benchmarks/selftest.yaml and "
            "its cases; regenerate with tools/packaging/generate_selftest_cases.py "
            "--write"
        )
        size = sum(path.stat().st_size for _, path in case_files())
        print(
            "PASS: the facade's selftest table names the files of the listed "
            f"cases, {size} bytes"
        )


if __name__ == "__main__":
    main()
