"""Run the 1.0 conversion gate on fresh installs (#3030 P1-9, D76).

The engine reads language version 1.0, so every benchmark must give the same
result written as 1.0. The converter (`tools.migrate`, P2-1) writes every
benchmark as 1.0 into a temporary directory, and public benchmark
conformance runs the conversion there, as the equivalence gate runs the
originals:

- each positive reproduces its expected bytes;
- each negative its expected first issue: the converter rewrites the spec
  paths of `expected/error.yaml` through the rename table's `paths` block
  and changes nothing else in it, which this gate checks first, so the
  issue keeps its phase, condition, rule and context;
- on Python, each Define benchmark composes its expected documents, or its
  expected first issue, at its fixed time.

A benchmark whose expected files a decided item changes is excepted
(`tools/migrate/excepted.py`) and listed with its reason. CI runs the gate on
Python for pull requests that reach the 1.0 front end, the converter or the
1.0 schemas, and on both hosts weekly; `--hosts python,r` runs both here.
"""

import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path

import yaml
from tools import migrate
from tools.benchmark import directories, load_metadata
from tools.commands import equivalence, r_tests
from tools.conformance import public_conformance
from tools.migrate.excepted import EXCEPTED
from tools.paths import ROOT

FIXTURES = ROOT / "benchmarks"
HOSTS = ("python", "r")
# A Define benchmark publishes its expected tables; the call composes the rest.
PUBLISHED = (".csv", ".parquet")


def convert(target):
    """Write every benchmark as 1.0 under `target`; return the converted
    benchmarks directory. The converter writes each file's bytes, so the
    corpus's symbolic links are made again as links."""
    result = migrate.convert([FIXTURES], base=ROOT)
    migrate.write(result, target, [FIXTURES])
    fixtures = target / FIXTURES.relative_to(ROOT)
    for link in sorted(FIXTURES.rglob("*")):
        if link.is_symlink():
            converted = fixtures / link.relative_to(FIXTURES)
            converted.unlink(missing_ok=True)
            converted.symlink_to(os.readlink(link))
    return fixtures


def read_yaml(path):
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def negative_paths(fixtures):
    """Compare each converted expected error with its original.

    Returns the benchmarks whose error changed in anything but its spec
    paths, and each spec path the conversion rewrote, as (benchmark,
    original, converted)."""
    changed, rewritten = [], []
    for original in sorted(FIXTURES.glob("*/expected/error.yaml")):
        name = original.parent.parent.name
        before = read_yaml(original)
        after = read_yaml(fixtures / name / "expected" / "error.yaml")
        paths = before.get("spec_paths") or [], after.get("spec_paths") or []
        rest = [
            {key: value for key, value in error.items() if key != "spec_paths"}
            for error in (before, after)
        ]
        if rest[0] != rest[1] or len(paths[0]) != len(paths[1]):
            changed.append(name)
            continue
        rewritten.extend((name, old, new) for old, new in zip(*paths) if old != new)
    return changed, rewritten


def except_benchmarks(fixtures):
    """Exclude each excepted benchmark from the converted corpus's plan."""
    for name, reason in sorted(EXCEPTED.items()):
        path = fixtures / name / "benchmark.yaml"
        metadata = read_yaml(path)
        metadata.setdefault("conformance", {})["excluded"] = (
            f"excepted by the 1.0 conversion gate: {reason}"
        )
        path.write_text(
            yaml.safe_dump(metadata, sort_keys=False, allow_unicode=False),
            encoding="utf-8",
        )


def define_manifest(fixtures):
    """The Define call of each converted benchmark that declares one and is
    not excepted: its project, section, fixed time, published tables, the
    documents it composes, and its expected first issue."""
    cases = []
    for case in directories(fixtures):
        define = load_metadata(case).get("define")
        if define is None or case.name in EXCEPTED:
            continue
        expected = sorted(path.name for path in (case / "expected").iterdir())
        error = case / "expected" / "error.yaml"
        cases.append(
            {
                "name": case.name,
                "project": define["environment"],
                "standard": define["standard"],
                "created": define["created"],
                "published": [name for name in expected if name.endswith(PUBLISHED)],
                "expected": [
                    name
                    for name in expected
                    if not name.endswith(PUBLISHED) and name != "error.yaml"
                ],
                "error": read_yaml(error) if error.is_file() else None,
            }
        )
    return {"cases": cases}


def python_define(work, python, fixtures):
    """Compose each Define benchmark on the installed Python host; return
    whether any differs."""
    work.mkdir(parents=True)
    manifest = work / "manifest.json"
    manifest.write_text(
        json.dumps(define_manifest(fixtures), indent=2, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )
    result = subprocess.run(
        [
            str(python),
            "-I",
            str(ROOT / "python/tests/installed/conversion_define.py"),
            "--manifest",
            str(manifest),
            "--fixtures",
            str(fixtures),
            "--output",
            str(work / "work"),
        ],
        cwd=work,
    )
    return result.returncode != 0


def report(converted, changed, rewritten):
    """Print what the conversion wrote and what the gate excepts."""
    print(f"converted {converted} benchmarks to 1.0 into a temporary directory")
    print(f"excepted {len(EXCEPTED)} benchmarks:")
    for name, reason in sorted(EXCEPTED.items()):
        print(f"  {name}: {reason}")
    print(
        f"negative spec paths the rename table's paths block rewrote: {len(rewritten)}"
    )
    for name, old, new in rewritten:
        print(f"  {name}: {old} -> {new}")
    for name in changed:
        print(f"  {name}: its expected error changed beyond its spec paths")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv", default="uv")
    parser.add_argument(
        "--hosts",
        default="python",
        help="comma-separated hosts to run: python, r (default: python)",
    )
    args = parser.parse_args(argv)
    hosts = [host.strip() for host in args.hosts.split(",") if host.strip()]
    if not hosts or any(host not in HOSTS for host in hosts):
        parser.error(f"--hosts names python, r or both, not {args.hosts!r}")
    revision = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    ).strip()
    with tempfile.TemporaryDirectory(prefix="yamaa-conversion-") as directory:
        work = Path(directory)
        fixtures = convert(work / "converted")
        changed, rewritten = negative_paths(fixtures)
        report(len(directories(fixtures)), changed, rewritten)
        if changed:
            raise SystemExit(
                "the conversion changed an expected error beyond its spec paths: "
                + ", ".join(changed)
            )
        except_benchmarks(fixtures)
        plan = work / "plan"
        public_conformance.main(
            ["plan", "--fixtures", str(fixtures), "--output", str(plan)]
        )
        failed = []
        if "python" in hosts:
            python = equivalence.install_python(work / "python-install", args.uv)
            if equivalence.python_conformance(
                work / "python", python, plan, revision, fixtures
            ):
                failed.append("Python public benchmark conformance")
            if python_define(work / "python-define", python, fixtures):
                failed.append("Python Define benchmarks")
        if "r" in hosts:
            _, environment = r_tests.install(work / "r-install")
            if equivalence.r_conformance(
                work / "r", environment, plan, revision, fixtures
            ):
                failed.append("R public benchmark conformance")
        if failed:
            raise SystemExit("1.0 conversion gate failed: " + "; ".join(failed))
    print(f"conversion gate passed on {', '.join(hosts)}")


if __name__ == "__main__":
    main()
