"""Compose each converted Define benchmark on the installed Python host.

`make check-conversion` (#3030 P1-9) runs this with `python -I` outside the
checkout, over the benchmarks it converted to 1.0. For each case of the
manifest it writes, the converted benchmark is copied, without its expected
files, into a working directory beside its published tables, and its section
is submitted at the benchmark's fixed time. A positive's saved documents must
equal its expected bytes; a negative's first issue must match its expected
error, and its save must write nothing. The manifest carries the calls and
the expected errors, so this runner reads no YAML.
"""

import argparse
import json
import shutil
import sys
from pathlib import Path

import yamaa

KEYS = ("phase", "condition", "spec_paths", "rule")


def judge(case, fixtures, output):
    """The findings of one case, empty when it reproduces its truth."""
    source = fixtures / case["name"]
    directory = output / case["name"]
    shutil.copytree(
        source,
        directory,
        symlinks=True,
        ignore=shutil.ignore_patterns("expected", "README.md", "benchmark.yaml"),
    )
    for name in case["published"]:
        shutil.copy2(source / "expected" / name, directory / name)
    result = yamaa.project(directory / case["project"]).submission(
        case["standard"], created=case["created"]
    )
    issues = result.issues.to_dicts()
    want = case["error"]
    if want is not None:
        if not issues:
            return ["the submission reported no issue"]
        first = dict(issues[0])
        first["context"] = json.loads(first["context"])
        findings = [
            {"key": key, "expected": want[key], "actual": first.get(key)}
            for key in KEYS
            if key in want and first.get(key) != want[key]
        ]
        findings.extend(
            {"context": key, "expected": value, "actual": first["context"].get(key)}
            for key, value in (want.get("context") or {}).items()
            if first["context"].get(key) != value
        )
        try:
            result.save()
            findings.append("a refused submission saved")
        except yamaa.Error:
            pass
        if (directory / "define.xml").exists():
            findings.append("a refused submission wrote define.xml")
        return findings
    if issues:
        return [{"unexpected_issues": issues}]
    paths = result.save()
    written = sorted(Path(path).name for path in paths)
    if written != sorted(case["expected"]):
        return [{"saved": written, "expected": case["expected"]}]
    return [
        f"saved bytes differ: {name}"
        for name in case["expected"]
        if (directory / name).read_bytes() != (source / "expected" / name).read_bytes()
    ]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    fixtures, output = args.fixtures.resolve(), args.output.resolve()
    if Path(yamaa.__file__).resolve().is_relative_to(fixtures.parent):
        raise SystemExit(
            "the Define runner needs a package installed outside the fixtures"
        )
    failures = {}
    for case in manifest["cases"]:
        try:
            findings = judge(case, fixtures, output)
        except (KeyboardInterrupt, SystemExit):
            raise
        except BaseException as error:  # noqa: BLE001 -- a native panic is a case's failure too
            findings = [f"{type(error).__name__}: {error}"]
        print(case["name"], "mismatch" if findings else "pass", flush=True)
        if findings:
            failures[case["name"]] = findings
    print(
        json.dumps(
            {"cases": len(manifest["cases"]), "failures": failures},
            indent=2,
            ensure_ascii=True,
        )
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
