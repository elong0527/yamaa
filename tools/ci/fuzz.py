"""Build and run the fuzz targets for the readers that parse untrusted input.

Each target in `rust/fuzz` feeds its input to an entry point the hosts use.
`run` seeds a target from the committed fixtures and its text regression
inputs, then fuzzes it for a fixed time; a crash, sanitizer report, panic,
timeout or memory blowup fails the run. CI runs the targets whose readers a
pull request changes, as `select` reports them, and every target on a nightly
schedule. A run continues from the corpus already under `rust/fuzz/corpus`;
CI restores main's last corpus there and, after a passing run on main,
`minimize` shrinks it to the inputs that keep its coverage before CI saves it
for the next run.
"""

import argparse
import gzip
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

from tools.paths import ROOT, WORKSPACE

FUZZ = WORKSPACE / "fuzz"
# cargo-fuzz builds with nightly-only sanitizer and coverage flags. A dated
# nightly and an exact cargo-fuzz keep runs reproducible; move them like any
# other pin. The workspace keeps its own toolchain.
NIGHTLY = "nightly-2026-10-15"
CARGO_FUZZ = "0.13.2"

# The cc crate reads CC and CFLAGS, and only this build sets them. The C that
# the fuzz build compiles, ReadStat and the Zstandard library behind the
# Parquet codec, carries AddressSanitizer, UndefinedBehaviorSanitizer with
# every report fatal, and the coverage libFuzzer steers by. Normal builds set
# neither variable, so they compile that C unchanged. The adapters' build
# script sees `-fsanitize` in CFLAGS and leaves the ReadStat C unfortified: a
# fortified call checks its own bounds and hides the access from
# AddressSanitizer.
SANITIZED_C = {
    "CC": "clang",
    "CFLAGS": "-fsanitize=address,undefined,fuzzer-no-link "
    "-fno-sanitize-recover=undefined -fno-omit-frame-pointer",
}

# cargo-fuzz compiles each crate as one unit by default. The engine crates
# are large and a fresh checkout rebuilds them on every CI run, so parallel
# code generation trades slightly slower executions for much shorter builds.
CODEGEN = ("--codegen-units", "16")

# libFuzzer limits for one input. Rust code runs with AddressSanitizer and
# overflow checks; a slower input than this is a hang.
TIMEOUT_SECONDS = 30
RSS_LIMIT_MB = 4096

# A change to these runs every target: the harness, its build, this tool, the
# reader crates' dependencies and features, the fixed limits every reader
# enforces, and the core table, value and source decode types that the readers
# build and the harness matches exhaustively.
SHARED = (
    ".github/workflows/fuzz.yml",
    "rust/Cargo.lock",
    "rust/crates/yamaa-adapters/Cargo.toml",
    "rust/crates/yamaa-core/Cargo.toml",
    "rust/crates/yamaa-core/src/decoder.rs",
    "rust/crates/yamaa-core/src/limits.rs",
    "rust/crates/yamaa-core/src/specification_run.rs",
    "rust/crates/yamaa-core/src/table.rs",
    "rust/crates/yamaa-core/src/value.rs",
    "rust/fuzz/Cargo.toml",
    "rust/fuzz/src/",
    "tools/ci/fuzz.py",
)
# The source decode that both host routes call for held bytes.
SOURCE_DECODE = (
    "rust/crates/yamaa-adapters/src/column_table.rs",
    "rust/crates/yamaa-adapters/src/columns.rs",
    "rust/crates/yamaa-adapters/src/decoder.rs",
    "rust/crates/yamaa-adapters/src/epoch.rs",
    "rust/crates/yamaa-adapters/src/source_decoding.rs",
    "rust/crates/yamaa-adapters/src/source_ordinal.rs",
    "rust/crates/yamaa-core/src/odm_source.rs",
    "rust/crates/yamaa-core/src/temporal.rs",
)
SAS = (
    *SOURCE_DECODE,
    "rust/crates/yamaa-adapters/build.rs",
    "rust/crates/yamaa-adapters/src/sas_source.rs",
    "rust/crates/yamaa-adapters/src/sas_source/",
    "rust/crates/yamaa-adapters/vendor/readstat/",
)
CSV = (
    "rust/crates/yamaa-adapters/src/column_table.rs",
    "rust/crates/yamaa-adapters/src/columns.rs",
    "rust/crates/yamaa-adapters/src/csv_source.rs",
    "rust/crates/yamaa-adapters/src/typed_csv.rs",
    "rust/crates/yamaa-core/src/conversion.rs",
    "rust/crates/yamaa-core/src/csv_source.rs",
    "rust/crates/yamaa-core/src/temporal.rs",
    "rust/crates/yamaa-core/src/typed_csv.rs",
)


@dataclass(frozen=True)
class Target:
    """One fuzz target: its seed fixtures, the readers it covers, its inputs."""

    # Git pathspecs of the committed fixtures that seed the corpus.
    seeds: tuple[str, ...]
    # Files, and directories ending in "/", whose change runs it on a PR. Its
    # own `rust/fuzz/fuzz_targets/<name>.rs` and regression inputs count too.
    readers: tuple[str, ...]
    # The longest input libFuzzer generates; it truncates longer seeds.
    max_len: int
    # The further seeds each fixture gives, by label, written after its copy:
    # the input a target reads that the fixture holds without being it.
    derived: Callable[[Path], dict[str, bytes]] | None = None


# A member name longer than the 100 bytes a ustar header holds, so the GNU and
# PAX archives below carry a long-name entry and a path record.
LONG_MEMBER = "study/" + "nested/" * 14 + "{name}"


def archives(path):
    """One fixture as the only member of a TAR archive in each header format
    the ODM reader reads (ustar, GNU and PAX), and of a gzip-compressed ustar
    archive. Fixed metadata and timestamps keep the bytes reproducible."""
    data = path.read_bytes()
    written = {}
    for label, form, member in (
        ("ustar", tarfile.USTAR_FORMAT, path.name),
        ("gnu", tarfile.GNU_FORMAT, LONG_MEMBER.format(name=path.name)),
        ("pax", tarfile.PAX_FORMAT, LONG_MEMBER.format(name=path.name)),
    ):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w", format=form) as archive:
            info = tarfile.TarInfo(member)
            info.size = len(data)
            info.mtime = 0
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(data))
        written[label] = buffer.getvalue()
    written["ustar-gzip"] = gzip.compress(written["ustar"], mtime=0)
    return written


# The longest study directory the `study` target reads, its bundled seeds
# included.
STUDY_BYTES = 131072


def regex_cases(path):
    """The portable regex conformance cases of `path` as the
    `portable_regex` target reads them: each pattern, then a NUL byte and its
    subject when the case names one. The fixture's own quoting is read here:
    a pattern is single-quoted, a subject double-quoted ASCII with escapes."""
    written = {}
    case = None
    for line in path.read_text(encoding="ascii").splitlines():
        if match := re.fullmatch(r"\s*- id: (\S+)", line):
            case = match.group(1)
        elif match := re.fullmatch(r"\s+pattern: '((?:[^']|'')*)'", line):
            written[case] = match.group(1).replace("''", "'").encode()
        elif match := re.fullmatch(r'\s+subject: "((?:[^"\\]|\\.)*)"', line):
            subject = match.group(1).encode("ascii").decode("unicode_escape")
            written[case] += b"\0" + subject.encode()
    return written


def directory(path):
    """The study directory of the specification `path`, as the `study`
    target reads one: the specification, then each other committed file
    below its directory as a NUL byte, its path from there, a line feed and
    its bytes. The case's description and authored expectations are left
    out, as is a file that holds a NUL byte or would pass `STUDY_BYTES`."""
    root = path.parent
    written = bytearray(path.read_bytes())
    for member in committed([root.relative_to(ROOT).as_posix() + "/"]):
        name = member.relative_to(root).as_posix()
        if member == path or name in ("README.md", "benchmark.yaml"):
            continue
        if name.startswith("expected/"):
            continue
        data = member.read_bytes()
        entry = b"\0" + name.encode() + b"\n" + data
        if b"\0" not in data and len(written) + len(entry) <= STUDY_BYTES:
            written += entry
    return {"directory": bytes(written)}


TARGETS = {
    # A foreign Arrow C stream that the input's bytes build, through both
    # imports; any bytes build one, so the Parquet fixtures seed it.
    "arrow_stream": Target(
        ("*.parquet",),
        (
            "rust/crates/yamaa-adapters/src/arrow_bridge.rs",
            "rust/crates/yamaa-adapters/src/column_table.rs",
            "rust/crates/yamaa-adapters/src/columns.rs",
            "rust/crates/yamaa-adapters/src/epoch.rs",
            "rust/crates/yamaa-adapters/src/public_table.rs",
            "rust/crates/yamaa-adapters/src/storage_api.rs",
        ),
        4096,
    ),
    "csv": Target(("*.csv",), (*SOURCE_DECODE, *CSV), 65536),
    # The Dataset-JSON writer over a CSV artifact, and the source decode of
    # each file it writes.
    "dataset_json": Target(
        ("*.csv",),
        (
            *SOURCE_DECODE,
            *CSV,
            "rust/crates/yamaa-adapters/src/dataset_json_source.rs",
            "rust/crates/yamaa-adapters/src/submission_json.rs",
        ),
        65536,
    ),
    # The positional reading of the csv profile, its layout from the input's
    # first bytes.
    "fixed_width": Target(("*.csv",), (*SOURCE_DECODE, *CSV), 65536),
    # The ODM XML reader with its TAR and gzip admission, and the table it
    # builds; then the source decode of a specification's `.xml` input.
    "odm_xml": Target(
        ("*.xml",),
        (
            *SOURCE_DECODE,
            "rust/crates/yamaa-adapters/src/odm_xml/",
            "rust/crates/yamaa-adapters/src/public_table.rs",
            "rust/crates/yamaa-core/src/odm_items.rs",
        ),
        65536,
        derived=archives,
    ),
    # The lock classification, the renv reader and uv lock verification with
    # its PEP 440 and PEP 508 readers.
    "packaging_lock": Target(
        ("*renv.lock", "*uv.lock", "*renv_lock.json"),
        (
            "rust/crates/yamaa-adapters/src/json_wire.rs",
            "rust/crates/yamaa-adapters/src/project_lock.rs",
            "rust/crates/yamaa-adapters/src/python_packaging/",
            "rust/crates/yamaa-adapters/src/yaml_decode.rs",
            "rust/crates/yamaa-core/src/json/",
        ),
        1048576,
    ),
    "parquet": Target(
        ("*.parquet",),
        (
            *SOURCE_DECODE,
            "rust/crates/yamaa-adapters/profile/",
            "rust/crates/yamaa-adapters/src/parquet_source/",
            "rust/crates/yamaa-adapters/src/storage_api.rs",
            "rust/crates/yamaa-core/src/parquet_source.rs",
        ),
        65536,
    ),
    # The regex subset's compilation and searches, seeded by the conformance
    # cases' patterns and subjects.
    "portable_regex": Target(
        ("yaml/conformance/regex.yaml",),
        ("rust/crates/yamaa-core/src/portable_regex/",),
        16384,
        derived=regex_cases,
    ),
    "sas7bdat": Target(("*.sas7bdat",), SAS, 262144),
    "specification": Target(
        ("*.yaml",),
        # The YAML reader, then schema admission, preparation and the
        # expression parser; the nightly run covers the rest of compilation.
        (
            "rust/crates/yamaa-adapters/schema/",
            "rust/crates/yamaa-adapters/src/shipped_schema/",
            "rust/crates/yamaa-adapters/src/specification_source/",
            "rust/crates/yamaa-adapters/src/yaml_decode.rs",
            "rust/crates/yamaa-adapters/src/yaml_decode/",
            "rust/crates/yamaa-adapters/src/yaml_source.rs",
            "rust/crates/yamaa-adapters/src/yaml_source/",
            "rust/crates/yamaa-application/Cargo.toml",
            "rust/crates/yamaa-application/src/specification_run.rs",
            "rust/crates/yamaa-core/src/dictionary_preparation.rs",
            "rust/crates/yamaa-core/src/domain.rs",
            "rust/crates/yamaa-core/src/expression/",
            "rust/crates/yamaa-core/src/inheritance.rs",
            "rust/crates/yamaa-core/src/inheritance_capture.rs",
            "rust/crates/yamaa-core/src/inheritance_preparation.rs",
            "rust/crates/yamaa-core/src/schema/",
            "rust/crates/yamaa-core/src/specification/",
            "rust/vendor/saphyr-parser/",
        ),
        65536,
    ),
    # A study directory checked and run on the file route: the project and
    # the files it names, compilation against them, and execution. Each
    # benchmark specification seeds it with its case's directory. Changes to
    # the route and to project preparation run it on a pull request; the
    # nightly run covers the rest of compilation and execution.
    "study": Target(
        ("benchmarks/*/spec.yaml",),
        (
            "rust/crates/yamaa-adapters/src/document.rs",
            "rust/crates/yamaa-adapters/src/file_resources.rs",
            "rust/crates/yamaa-adapters/src/host_resources.rs",
            "rust/crates/yamaa-adapters/src/project_source.rs",
            "rust/crates/yamaa-application/src/",
            "rust/crates/yamaa-core/src/build.rs",
            "rust/crates/yamaa-core/src/expression/",
            "rust/crates/yamaa-core/src/file_preparation.rs",
            "rust/crates/yamaa-core/src/project_call_binding.rs",
            "rust/crates/yamaa-core/src/project_codelist_binding.rs",
            "rust/crates/yamaa-core/src/project_domain.rs",
            "rust/crates/yamaa-core/src/project_environment.rs",
            "rust/crates/yamaa-core/src/project_function.rs",
            "rust/crates/yamaa-core/src/project_function_document.rs",
            "rust/crates/yamaa-core/src/project_layers.rs",
            "rust/crates/yamaa-core/src/project_preparation.rs",
        ),
        STUDY_BYTES,
        derived=directory,
    ),
    "xport": Target(("*.xpt",), SAS, 65536),
}


def readers(name):
    """Every path whose change runs one target on a pull request."""
    return (
        *SHARED,
        f"rust/fuzz/fuzz_targets/{name}.rs",
        f"rust/fuzz/regressions/{name}/",
        *TARGETS[name].readers,
    )


def selected(changed):
    """The targets whose readers include one of the changed paths."""
    changed = tuple(changed)
    return [
        name
        for name in TARGETS
        if any(
            path == reader or (reader.endswith("/") and path.startswith(reader))
            for path in changed
            for reader in readers(name)
        )
    ]


def committed(pathspecs):
    """Committed files matching the pathspecs, in path order."""
    listing = subprocess.run(
        ["git", "ls-files", "-z", "--", *pathspecs],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout.decode()
    return sorted(ROOT / name for name in listing.split("\0") if name)


def regression(path):
    """Decode one text regression input: hex digits, `#` comment lines."""
    lines = path.read_text(encoding="ascii").splitlines()
    return bytes.fromhex("".join(line for line in lines if not line.startswith("#")))


def regressions(name):
    return sorted((FUZZ / "regressions" / name).glob("*.hex"))


def seed(name, destination):
    """Write one target's seeds to a new directory: committed fixtures first,
    each followed by the seeds the target derives from it, then its
    regression inputs. Names keep no extension, so no seed reads as a
    repository source file."""
    destination.mkdir(parents=True)
    target = TARGETS[name]
    fixtures = committed(target.seeds)
    for index, path in enumerate(fixtures):
        label = re.sub(r"[^A-Za-z0-9]+", "-", path.relative_to(ROOT).as_posix())
        shutil.copyfile(path, destination / f"{index:04d}-{label}")
        if target.derived:
            for kind, data in target.derived(path).items():
                kind = re.sub(r"[^A-Za-z0-9]+", "-", kind)
                (destination / f"{index:04d}-{label}-{kind}").write_bytes(data)
    for path in regressions(name):
        (destination / f"regression-{path.stem}").write_bytes(regression(path))
    return len(fixtures)


def copy_lock():
    """Copy the canonical lock into the fuzz workspace, so the engine builds
    on the reviewed versions and only the fuzzing crates resolve, at the
    exact versions the fuzz manifest pins."""
    shutil.copyfile(WORKSPACE / "Cargo.lock", FUZZ / "Cargo.lock")


def cargo_fuzz(command, *arguments):
    """Run cargo-fuzz from rust/ with the pinned nightly, on a copy of the
    canonical lock and with the C sanitizers."""
    copy_lock()
    return subprocess.run(
        ["cargo", f"+{NIGHTLY}", "fuzz", command, *CODEGEN, *arguments],
        cwd=WORKSPACE,
        env={**os.environ, **SANITIZED_C},
    ).returncode


def install():
    subprocess.run(
        [
            "rustup",
            "toolchain",
            "install",
            NIGHTLY,
            "--profile",
            "minimal",
            "--no-self-update",
        ],
        check=True,
    )
    subprocess.run(
        [
            "cargo",
            f"+{NIGHTLY}",
            "install",
            "cargo-fuzz",
            "--version",
            CARGO_FUZZ,
            "--locked",
        ],
        cwd=ROOT,
        check=True,
    )


def limits(name):
    """The libFuzzer limits for one input of a target, for runs and merges."""
    return (
        f"-max_len={TARGETS[name].max_len}",
        f"-timeout={TIMEOUT_SECONDS}",
        f"-rss_limit_mb={RSS_LIMIT_MB}",
    )


def run(name, seconds):
    """Seed a target afresh and fuzz it for `seconds`, on top of the corpus
    a previous run left; return the exit code."""
    seeds = FUZZ / "seeds" / name
    shutil.rmtree(seeds, ignore_errors=True)
    count = seed(name, seeds)
    print(
        f"{name}: {count} committed fixtures and {len(regressions(name))} regressions"
    )
    corpus = FUZZ / "corpus" / name
    corpus.mkdir(parents=True, exist_ok=True)
    # The first directory collects new inputs; the seeds stay as written.
    return cargo_fuzz(
        "run",
        name,
        str(corpus),
        str(seeds),
        "--",
        f"-max_total_time={seconds}",
        *limits(name),
        "-print_final_stats=1",
    )


def minimize(name):
    """Replace a target's corpus by its smallest subset with the same
    coverage, so the copy CI keeps between runs stays small; return the
    exit code."""
    corpus = FUZZ / "corpus" / name
    if not corpus.is_dir() or not any(corpus.iterdir()):
        print(f"{name}: no corpus to minimize")
        return 0
    return cargo_fuzz("cmin", name, str(corpus), "--", *limits(name))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("install", help="install the pinned nightly and cargo-fuzz")
    select = commands.add_parser(
        "select", help="print the targets to run as a JSON list for CI"
    )
    select.add_argument(
        "--since",
        help="select only targets whose readers changed since this revision",
    )
    commands.add_parser("build", help="build every fuzz target")
    fuzz = commands.add_parser("run", help="seed and fuzz one target")
    fuzz.add_argument("target", choices=TARGETS)
    fuzz.add_argument("--seconds", type=int, default=60)
    shrink = commands.add_parser(
        "minimize",
        help="shrink one target's corpus to the inputs that keep its coverage",
    )
    shrink.add_argument("target", choices=TARGETS)
    seeds = commands.add_parser("seeds", help="write one target's seeds")
    seeds.add_argument("target", choices=TARGETS)
    seeds.add_argument("destination", type=Path, help="a new directory")
    args = parser.parse_args()
    if args.command == "install":
        install()
    elif args.command == "select":
        names = list(TARGETS)
        if args.since:
            # Without rename detection a moved reader lists both of its paths.
            changed = subprocess.run(
                [
                    "git",
                    "diff",
                    "--name-only",
                    "--no-renames",
                    "-z",
                    args.since,
                    "HEAD",
                ],
                cwd=ROOT,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.split("\0")
            names = selected([path for path in changed if path])
        print(f"targets={json.dumps(names)}")
    elif args.command == "build":
        sys.exit(cargo_fuzz("build"))
    elif args.command == "run":
        sys.exit(run(args.target, args.seconds))
    elif args.command == "minimize":
        sys.exit(minimize(args.target))
    else:
        count = seed(args.target, args.destination)
        print(f"{args.target}: wrote {count} fixtures to {args.destination}")


if __name__ == "__main__":
    main()
