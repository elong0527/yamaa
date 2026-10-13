"""Plan and judge installed public runs without importing an evaluator.

Only the comparator reads independent expected truth. CSV bytes and typed host
views compare exactly; Parquet compares logical fields, types, nulls and rows.
"""

from __future__ import annotations

import argparse
import ast
import collections
import csv
import datetime as dt
import json
import re
import shutil
import struct
from pathlib import Path

PROTOCOL = "public-conformance/1"
EXCLUDED = {
    "negative-path-absolute": "reference-only path history (#1905)",
    "negative-path-parent-escape": "reference-only path history (#1905)",
}
RESOLVED_REASON = "reference-only resolved specification until cutover (#1973.14)"
LOG_INTS = {"EVALUATED_COUNT", "FAILURE_COUNT"}


def platform_matches(runtime, platform, recorded):
    """Validate each host's own platform spelling against the approved lane."""
    if not isinstance(recorded, str):
        return False
    if runtime == "python":
        prefixes = {
            "ubuntu-24.04": "Linux-",
            "macos-15": "macOS-",
            "windows-latest": "Windows-",
        }
        return platform in prefixes and recorded.startswith(prefixes[platform])
    if runtime == "r":
        patterns = {
            "ubuntu-24.04": r"x86_64-(?:pc|unknown)-linux-gnu",
            "macos-15": r"aarch64-apple-darwin[0-9]+",
            "windows-latest": r"x86_64-w64-mingw32",
        }
        return (
            platform in patterns
            and re.fullmatch(patterns[platform], recorded) is not None
        )
    return False


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def python_entries(path):
    entries = []
    for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
        if not (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == "yamaa"
            and node.func.attr in {"domain", "yamaa_domain"}
        ):
            continue
        if len(node.args) != 1 or any(k.arg != "environment" for k in node.keywords):
            raise ValueError(f"unsupported runner arguments: {path}")
        specification = ast.literal_eval(node.args[0])
        environment = (
            ast.literal_eval(node.keywords[0].value) if node.keywords else None
        )
        if not isinstance(specification, str) or (
            environment is not None and not isinstance(environment, str)
        ):
            raise ValueError(f"runner paths must be literal strings: {path}")
        entries.append(
            (node.lineno, {"specification": specification, "environment": environment})
        )
    if not entries:
        raise ValueError(f"no public domain entry in {path}")
    return [entry for _, entry in sorted(entries)]


def r_entries(path):
    # Reject a changed runner shape instead of guessing its invocation.
    source = "\n".join(
        line.split("#", 1)[0] for line in path.read_text(encoding="utf-8").splitlines()
    )
    pattern = (
        r'yamaa_domain\(\s*"([^"\n]+)"\s*(?:,\s*environment\s*=\s*"([^"\n]+)"\s*)?\)'
    )
    calls = list(re.finditer(pattern, source))
    if not calls or len(calls) != len(re.findall(r"\byamaa_domain\s*\(", source)):
        raise ValueError(f"unsupported literal R runner: {path}")
    return [{"specification": c.group(1), "environment": c.group(2)} for c in calls]


def plan(fixtures):
    examples = []
    for case in sorted(fixtures.iterdir()):
        if not case.is_dir() or not list(case.glob("spec*.yaml")):
            continue
        entries = {}
        for runtime, filename, extractor in (
            ("python", "run.py", python_entries),
            ("r", "run.R", r_entries),
        ):
            if (case / filename).exists():
                entries[runtime] = extractor(case / filename)
            else:
                specs = list(case.glob("spec*.yaml"))
                if len(specs) != 1:
                    raise ValueError(f"ambiguous negative entry: {case}")
                environment = f"{runtime}/environment.yaml"
                entries[runtime] = [
                    {
                        "specification": specs[0].name,
                        "environment": environment
                        if (case / environment).is_file()
                        else None,
                    }
                ]
        examples.append(
            {"name": case.name, "entries": entries, "excluded": EXCLUDED.get(case.name)}
        )
    return {"protocol": PROTOCOL, "examples": examples}


def emit_plan(fixtures, output):
    if output.exists():
        raise ValueError("plan output must be a fresh directory")
    payload = plan(fixtures)
    write_json(output / "plan.json", payload)
    # Base R cannot inspect symbolic links on Windows. Stage native inputs with
    # Python's symlink-aware copy, then let R rename each case without copying
    # through its target. Independent expected truth never enters this tree.
    stage_r_inputs(fixtures, output / "r-inputs", payload)
    for runtime in ("python", "r"):
        with (output / f"{runtime}.tsv").open(
            "w", encoding="utf-8", newline=""
        ) as stream:
            writer = csv.writer(
                stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE
            )
            writer.writerow(["example", "specification", "environment", "excluded"])
            for example in payload["examples"]:
                for entry in example["entries"][runtime]:
                    writer.writerow(
                        [
                            example["name"],
                            entry["specification"],
                            entry["environment"] or "",
                            example["excluded"] or "",
                        ]
                    )
    print(f"planned {len(payload['examples'])} benchmarks for Python and R")


def stage_r_inputs(fixtures, output, payload):
    if output.exists():
        raise ValueError("R input staging must be a fresh directory")
    output.mkdir(parents=True)
    for example in payload["examples"]:
        if example["excluded"]:
            continue
        shutil.copytree(
            fixtures / example["name"],
            output / example["name"],
            symlinks=True,
            ignore=shutil.ignore_patterns("expected", "run.py", "run.R", "README.md"),
        )


def load_yaml(path):
    import yaml12

    return yaml12.parse_yaml(path.read_text(encoding="utf-8"))


def declarations(path, active=()):
    """Read independent declared types/output paths, never derivations."""
    if path.resolve() in active:
        raise ValueError("cyclic declarations")
    data = load_yaml(path)
    if not isinstance(data, dict):
        raise TypeError("specification declarations are not a mapping")
    types, output = {}, {}
    parents = data.get("parents", [])
    if isinstance(parents, str):
        parents = [parents]
    for parent in parents:
        inherited_types, inherited_output = declarations(
            path.parent / parent, (*active, path.resolve())
        )
        types.update(inherited_types)
        output.update(inherited_output)
    for column in data.get("columns", []):
        if "type" in column:
            types[column["name"]] = column["type"]
    output.update(data.get("output", {}))
    return types, output


def scalar(value, kind):
    if value is None:
        return None
    if kind == "str":
        return str(value).encode("utf-8").hex()
    if kind == "int":
        return str(int(value))
    if kind == "float":
        return struct.pack(">d", float(value)).hex()
    if kind == "date":
        date = value if isinstance(value, dt.date) else dt.date.fromisoformat(value)
        return str((date - dt.date(1970, 1, 1)).days)
    if kind == "datetime":
        date = (
            value
            if isinstance(value, dt.datetime)
            else dt.datetime.fromisoformat(value)
        )
        return str(
            int(
                (
                    date.replace(tzinfo=dt.timezone.utc)
                    - dt.datetime(1970, 1, 1, tzinfo=dt.timezone.utc)
                ).total_seconds()
            )
        )
    raise ValueError(f"unknown declared type: {kind}")


def csv_cells(path):
    # Retain quoted empty text versus an unquoted missing field.
    text = path.read_bytes().decode("utf-8")
    rows, row, field, quoted, in_quotes, start = [], [], [], False, False, True
    index = 0
    while index < len(text):
        char = text[index]
        if in_quotes:
            if char == '"' and index + 1 < len(text) and text[index + 1] == '"':
                field.append('"')
                index += 1
            elif char == '"':
                in_quotes = False
            else:
                field.append(char)
        elif char == '"' and start:
            quoted = in_quotes = True
            start = False
        elif char in ",\r\n":
            value = "".join(field)
            row.append(value if value or quoted else None)
            field, quoted, start = [], False, True
            if char != ",":
                rows.append(row)
                row = []
                if char == "\r" and index + 1 < len(text) and text[index + 1] == "\n":
                    index += 1
        else:
            field.append(char)
            start = False
        index += 1
    if field or quoted or row:
        value = "".join(field)
        row.append(value if value or quoted else None)
        rows.append(row)
    return rows


def expected_table(path, types):
    if path.suffix == ".parquet":
        import pyarrow as pa
        import pyarrow.parquet as pq

        table = pq.read_table(path)
        kinds = []
        for field in table.schema:
            kind = (
                "str"
                if pa.types.is_string(field.type)
                or pa.types.is_large_string(field.type)
                else "int"
                if pa.types.is_int64(field.type)
                else "float"
                if pa.types.is_float64(field.type)
                else "date"
                if pa.types.is_date32(field.type)
                else "datetime"
                if field.type == pa.timestamp("us")
                else None
            )
            if kind is None:
                raise ValueError(f"unexpected logical Parquet type: {field.type}")
            if types[field.name] != kind:
                raise ValueError(
                    f"independent Parquet type disagrees with declaration: {field.name}"
                )
            kinds.append(kind)
        columns = table.column_names
        rows = [
            [scalar(row[column], kind) for column, kind in zip(columns, kinds)]
            for row in table.to_pylist()
        ]
    else:
        contents = csv_cells(path)
        columns = contents[0]
        kinds = [types[column] for column in columns]
        if any(len(row) != len(columns) for row in contents[1:]):
            raise ValueError("independent CSV has a ragged row")
        rows = [
            [scalar(value, kind) for value, kind in zip(row, kinds)]
            for row in contents[1:]
        ]
    return {"columns": list(map(list, zip(columns, kinds))), "rows": rows}


def reported_table(table, decimals):
    """Independently project binary64 using REQ-0747, with no tolerance.

    Committed precision CSVs contain reported values. Use integer ratios so
    ties round away from zero exactly, rather than a host round() builtin.
    """
    if table is None:
        return None
    rows = []
    for row in table["rows"]:
        projected = []
        for value, (_, kind) in zip(row, table["columns"]):
            if value is None or kind != "float":
                projected.append(value)
                continue
            number = struct.unpack(">d", bytes.fromhex(value))[0]
            numerator, denominator = number.as_integer_ratio()
            rounded, remainder = divmod(abs(numerator) * 10**decimals, denominator)
            rounded += 2 * remainder >= denominator
            digits = str(rounded).zfill(decimals + 1)
            text = (
                digits
                if decimals == 0
                else digits[:-decimals] + "." + digits[-decimals:]
            )
            if numerator < 0 and rounded:
                text = "-" + text
            projected.append(struct.pack(">d", float(text)).hex())
        rows.append(projected)
    return {"columns": table["columns"], "rows": rows}


def subset(want, got):
    if isinstance(want, dict):
        return isinstance(got, dict) and all(
            k in got and subset(v, got[k]) for k, v in want.items()
        )
    return want == got


def diagnostic_matches(want, got):
    return (
        all(
            got.get(field) == want[field]
            for field in ("phase", "condition", "spec_paths")
        )
        and ("requirement" not in want or got.get("requirement") == want["requirement"])
        and subset(want.get("context") or {}, got.get("context") or {})
    )


def parquet_signature(path):
    import pyarrow.parquet as pq

    schema = pq.ParquetFile(path).schema
    return [
        (
            schema.column(index).name,
            schema.column(index).physical_type,
            str(schema.column(index).logical_type),
            schema.column(index).max_definition_level,
            schema.column(index).max_repetition_level,
        )
        for index in range(len(schema))
    ]


def log_types(path):
    if path.suffix == ".csv":
        columns = csv_cells(path)[0]
    else:
        import pyarrow.parquet as pq

        columns = pq.read_schema(path).names
    return {c: "int" if c in LOG_INTS else "str" for c in columns}


def compare_example(example, runtime, record, fixtures, observed):
    result = {
        "example": example["name"],
        "verdict": "error",
        "unsupported_operations": [],
        "findings": [],
        "not_compared": [],
    }
    if example["excluded"]:
        result.update(verdict="excluded", reason=example["excluded"])
        return result
    case = fixtures / example["name"]
    if (case / "expected/spec_resolved.yaml").exists():
        result["not_compared"].append(
            {"artifact": "spec_resolved.yaml", "reason": RESOLVED_REASON}
        )
    for filename in ("define.xml", "dm.json"):
        if (case / "expected" / filename).exists():
            result["not_compared"].append(
                {
                    "artifact": filename,
                    "reason": "Define not compared by domain runner; fixed-time public Define suite is separate",
                }
            )
    entries = example["entries"][runtime]
    if record.get("error"):
        result["findings"].append(record["error"])
        return result
    builds = record.get("builds", [])
    if [(b.get("specification"), b.get("environment")) for b in builds] != [
        (e["specification"], e["environment"]) for e in entries
    ]:
        result["findings"].append(
            "observed entry specifications/environments do not match the runner plan"
        )
        return result
    issues = [issue for build in builds for issue in build["issues"]]
    if any(i["phase"] == "engine" for i in issues):
        result["findings"].append({"engine_issues": issues})
        return result
    operations = [
        i.get("context", {}).get("operation")
        for i in issues
        if i["condition"] == "unsupported_operation"
    ]
    if operations:
        if any(op is None for op in operations):
            result["findings"].append(
                "unsupported_operation issue omitted its operation"
            )
            return result
        result.update(
            verdict="unsupported", unsupported_operations=sorted(set(operations))
        )
        return result
    negative = case / "expected/error.yaml"
    if negative.exists():
        want = load_yaml(negative)
        if not issues or not diagnostic_matches(want, issues[0]):
            result["findings"].append(
                {
                    "diagnostic": "first issue differs",
                    "expected": want,
                    "actual": issues[0] if issues else None,
                }
            )
        if any(b["tables"].get("output") is not None or b.get("saved") for b in builds):
            result["findings"].append("negative build exposed or saved output")
        result["verdict"] = "mismatch" if result["findings"] else "pass"
        return result
    if issues:
        result.update(verdict="mismatch")
        result["findings"].append({"unexpected_issues": issues})
        return result
    compared = set()
    for entry, build in zip(entries, builds):
        types, output = declarations(case / entry["specification"])
        for slot, key in (
            ("output", "path"),
            ("verification_log", "verification_log"),
            ("warning_log", "warning_log"),
        ):
            declared = output.get(key)
            actual = build["tables"].get(slot)
            if declared is None:
                if actual is not None:
                    result["findings"].append(f"undeclared {slot} host view")
                continue
            target = Path(entry["specification"]).parent / declared
            truth = case / "expected" / Path(declared).name
            candidate = observed / "work" / case.name / target
            compared.add(truth.name)
            if not truth.is_file():
                result["findings"].append(
                    f"declared {slot} lacks independent expected artifact: {declared}"
                )
                continue
            expected_types = types if slot == "output" else log_types(truth)
            expected = expected_table(truth, expected_types)
            if (
                slot == "output"
                and truth.suffix == ".csv"
                and output.get("decimals") is not None
            ):
                actual = reported_table(actual, output["decimals"])
                result["not_compared"].append(
                    {
                        "artifact": truth.name,
                        "reason": "unrounded float values: independent CSV supplies declared reported precision only (REQ-0747)",
                    }
                )
            if actual != expected:
                result["findings"].append(
                    {
                        "typed_table": slot,
                        "artifact": declared,
                        "expected": expected,
                        "actual": actual,
                    }
                )
            if not candidate.is_file():
                result["findings"].append(f"save did not publish {declared}")
            elif (
                truth.suffix == ".csv" and candidate.read_bytes() != truth.read_bytes()
            ):
                result["findings"].append(f"saved CSV bytes differ: {declared}")
            elif truth.suffix == ".parquet":
                if parquet_signature(candidate) != parquet_signature(truth):
                    result["findings"].append(
                        f"saved Parquet logical schema differs: {declared}"
                    )
                elif expected_table(candidate, expected_types) != expected:
                    result["findings"].append(
                        f"saved Parquet logical table differs: {declared}"
                    )
        if not build.get("saved"):
            result["findings"].append(
                f"successful {entry['specification']} did not save"
            )
    for truth in (case / "expected").iterdir():
        if truth.suffix in {".csv", ".parquet"} and truth.name not in compared:
            result["findings"].append(
                f"independent table artifact was not compared: {truth.name}"
            )
    result["verdict"] = "mismatch" if result["findings"] else "pass"
    return result


def compare(fixtures, observed, output, ratchet=None):
    meta = read_json(observed / "run.json")
    if meta["protocol"] != PROTOCOL or meta["runtime"] not in {"python", "r"}:
        raise ValueError("unknown runner protocol/runtime")
    runtime = meta["runtime"]
    payload = plan(fixtures)
    results = []
    for example in payload["examples"]:
        try:
            path = observed / "records" / (example["name"] + ".json")
            record = (
                read_json(path)
                if path.is_file()
                else {"error": "missing installed runner record"}
            )
            result = compare_example(example, runtime, record, fixtures, observed)
        except Exception as error:  # noqa: BLE001 -- retain one record for every benchmark
            result = {
                "example": example["name"],
                "verdict": "error",
                "findings": [f"{type(error).__name__}: {error}"],
                "unsupported_operations": [],
                "not_compared": [],
            }
        results.append(result)
    counts = dict.fromkeys(("pass", "mismatch", "unsupported", "error", "excluded"), 0)
    unsupported = collections.Counter()
    for result in results:
        counts[result["verdict"]] += 1
        unsupported.update(result["unsupported_operations"])
    inventory = {
        **meta,
        "summary": counts,
        "unsupported_operations": dict(unsupported.most_common()),
        "benchmarks": results,
        "ratchet": {"mode": "report-only"},
    }
    status = 0
    if ratchet:
        required = set(read_json(ratchet)[runtime])
        passed = {r["example"] for r in results if r["verdict"] == "pass"}
        inventory["ratchet"] = {
            "mode": "gate",
            "regressions": sorted(required - passed),
            "new_passes": sorted(passed - required),
        }
        status = int(bool(required - passed))
    write_json(output, inventory)
    print(
        json.dumps(
            {"runtime": runtime, **counts, "ratchet": inventory["ratchet"]}, indent=2
        )
    )
    return status


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    planner = sub.add_parser("plan")
    planner.add_argument("--fixtures", type=Path, required=True)
    planner.add_argument("--output", type=Path, required=True)
    judge = sub.add_parser("compare")
    judge.add_argument("--fixtures", type=Path, required=True)
    judge.add_argument("--observed", type=Path, required=True)
    judge.add_argument("--output", type=Path, required=True)
    judge.add_argument("--ratchet", type=Path)
    args = parser.parse_args(argv)
    if args.command == "plan":
        emit_plan(args.fixtures, args.output)
        return 0
    return compare(args.fixtures, args.observed, args.output, args.ratchet)


if __name__ == "__main__":
    raise SystemExit(main())
