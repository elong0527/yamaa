"""Plan and judge installed public runs without importing an evaluator.

Only the comparator reads independent expected truth. CSV bytes and typed host
views compare exactly; Parquet compares logical fields, types, nulls and rows.
A run's output and its declared run report are the compared tables. R holds a
declared integer as a plain double, which must equal that integer exactly.
"""

from __future__ import annotations

import argparse
import collections
import csv
import datetime as dt
import json
import math
import re
import shutil
import struct
from pathlib import Path

from tools.benchmark import directories as benchmark_directories
from tools.benchmark import entries as benchmark_entries
from tools.benchmark import load_metadata
from tools.paths import ROOT

PROTOCOL = "public-conformance/1"
FIELD_REFERENCE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*$")
RESOLVED_REASON = "reference-only resolved specification until cutover (#1973.14)"
# The run report's integer columns (CHECK-24); every other column is text.
REPORT_INTS = {"COUNT", "TOTAL"}
# R doubles hold every integer up to this magnitude exactly.
EXACT_DOUBLE = 2**53


def platform_matches(runtime, platform, recorded):
    """Validate each host's own platform spelling against the approved lane."""
    if not isinstance(recorded, str):
        return False
    if runtime == "python":
        prefixes = {
            "ubuntu-24.04": "Linux-",
            "macos-15": "macOS-",
            "windows-2025": "Windows-",
        }
        return platform in prefixes and recorded.startswith(prefixes[platform])
    if runtime == "r":
        patterns = {
            "ubuntu-24.04": r"x86_64-(?:pc|unknown)-linux-gnu",
            "macos-15": r"aarch64-apple-darwin[0-9]+",
            "windows-2025": r"x86_64-w64-mingw32",
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


def plan(fixtures):
    examples = []
    for case in benchmark_directories(fixtures):
        metadata = load_metadata(case)
        entries = benchmark_entries(case, metadata)
        example = {
            "name": case.name,
            "entries": entries,
            "excluded": metadata.get("conformance", {}).get("excluded"),
            "define": metadata.get("define"),
        }
        example["variants"], _ = example_variants(fixtures, example)
        examples.append(example)
    return {"protocol": PROTOCOL, "examples": examples}


def json_round_trippable(value):
    """True when a parsed document survives JSON serialization unchanged."""
    if value is None or isinstance(value, (str, bool, int)):
        return True
    if isinstance(value, float):
        return math.isfinite(value)
    if isinstance(value, list):
        return all(json_round_trippable(item) for item in value)
    if isinstance(value, dict):
        return all(
            isinstance(key, str) and json_round_trippable(item)
            for key, item in value.items()
        )
    return False


def key_order_texts(document):
    """JSON control/permuted texts for the key-order check, or None.

    Eligible documents have no `rows` and no `parent`, at least two keys,
    and every key column reading one input field or a literal, so key columns
    may be declared in any order (COLUMN-5, OUTPUT-1, OUTPUT-2).
    """
    if (
        not isinstance(document, dict)
        or "rows" in document
        or document.get("parent") is not None
    ):
        return None
    keys = document.get("keys") or []
    columns = document.get("columns") or []
    if len(keys) < 2 or not all(isinstance(key, str) for key in keys):
        return None
    if not json_round_trippable(document):
        return None
    by_name = {}
    for column in columns:
        if not isinstance(column, dict) or not isinstance(column.get("name"), str):
            return None
        by_name.setdefault(column["name"], column)
    for key in keys:
        column = by_name.get(key)
        if column is None:
            return None
        derivation = column.get("derivation")
        if isinstance(derivation, str):
            if not FIELD_REFERENCE.fullmatch(derivation):
                return None
        elif not (isinstance(derivation, dict) and set(derivation) == {"literal"}):
            return None
    control = json.dumps(document, indent=2, ensure_ascii=True) + "\n"
    permuted = json.loads(control)
    names = [column["name"] for column in permuted["columns"]]
    permuted["columns"] = [
        permuted["columns"][names.index(name)] for name in reversed(keys)
    ] + [column for column in permuted["columns"] if column["name"] not in keys]
    return control, json.dumps(permuted, indent=2, ensure_ascii=True) + "\n"


def example_variants(fixtures, example):
    """Plan one example's metamorphic variants and their file contents.

    Returns (descriptors, contents): descriptors maps each runtime to its
    variant list; contents maps each variant specification path, relative to
    the staged case directory, to the text to stage there. Every entry gets
    an empty child (LAYER-5); positive entries may also get the key-order
    control and permutation.
    """
    descriptors = {"python": [], "r": []}
    contents = {}
    if example["excluded"]:
        return descriptors, contents
    case = fixtures / example["name"]
    # A Define benchmark's domain builds succeed; its expected failure, if
    # any, belongs to the Define call the fixed-time Define suites run.
    negative = (case / "expected" / "error.yaml").exists() and not example.get("define")
    for runtime in ("python", "r"):
        for index, entry in enumerate(example["entries"][runtime]):
            spec = Path(entry["specification"])
            child = (spec.parent / f"__child_{spec.stem}.yaml").as_posix()
            descriptors[runtime].append(
                {"kind": "empty_child", "entry": index, "specification": child}
            )
            contents[child] = f'schema_version: "1.0"\nparent: {spec.name}\n'
            if negative:
                continue
            try:
                document = load_yaml(case / entry["specification"])
            except Exception:  # noqa: BLE001 -- an unreadable spec has no key-order variant
                document = None
            texts = key_order_texts(document)
            if texts is None:
                continue
            control = (spec.parent / f"__keyorder_control_{spec.stem}.yaml").as_posix()
            permuted = (
                spec.parent / f"__keyorder_permuted_{spec.stem}.yaml"
            ).as_posix()
            descriptors[runtime].append(
                {"kind": "key_order_control", "entry": index, "specification": control}
            )
            descriptors[runtime].append(
                {
                    "kind": "key_order_permuted",
                    "entry": index,
                    "specification": permuted,
                }
            )
            contents[control], contents[permuted] = texts
    return descriptors, contents


def emit_plan(fixtures, output):
    if output.exists():
        raise ValueError("plan output must be a fresh directory")
    payload = plan(fixtures)
    write_json(output / "plan.json", payload)
    # Base R cannot inspect symbolic links on Windows. Stage native inputs with
    # Python's symlink-aware copy, then let R rename each case without copying
    # through its target. Independent expected truth never enters this tree.
    stage_r_inputs(fixtures, output / "r-inputs", payload)
    # Variant saves must be isolated from base artifacts. Both execution trees
    # retain links; base R's recursive file.copy follows their targets.
    stage_r_inputs(fixtures, output / "r-variant-inputs", payload)
    # Metamorphic variant specifications are generated, never authored: the
    # Python runner copies them from the plan tree, the R runner receives them
    # inside its staged inputs.
    for example in payload["examples"]:
        if example["excluded"]:
            continue
        _, contents = example_variants(fixtures, example)
        for relative, text in contents.items():
            for base in (
                output / "variants",
                output / "r-inputs",
                output / "r-variant-inputs",
            ):
                target = base / example["name"] / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(text, encoding="utf-8")
    for runtime in ("python", "r"):
        with (output / f"{runtime}.tsv").open(
            "w", encoding="utf-8", newline=""
        ) as stream:
            writer = csv.writer(
                stream, delimiter="\t", lineterminator="\n", quoting=csv.QUOTE_NONE
            )
            writer.writerow(
                [
                    "example",
                    "specification",
                    "environment",
                    "excluded",
                    "variant",
                    "entry",
                ]
            )
            for example in payload["examples"]:
                for entry in example["entries"][runtime]:
                    writer.writerow(
                        [
                            example["name"],
                            entry["specification"],
                            entry["environment"] or "",
                            example["excluded"] or "",
                            "",
                            "",
                        ]
                    )
                for variant in example["variants"][runtime]:
                    entry = example["entries"][runtime][variant["entry"]]
                    writer.writerow(
                        [
                            example["name"],
                            variant["specification"],
                            entry["environment"] or "",
                            example["excluded"] or "",
                            variant["kind"],
                            variant["entry"],
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
            ignore=shutil.ignore_patterns("expected", "README.md", "benchmark.yaml"),
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
    parent = data.get("parent")
    if parent is not None:
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
    """Independently project binary64 using CSV-9 and FORMULA-16's interval.

    Committed precision CSVs contain reported values. Use integer ratios so
    ties, and near ties within 2^-26 below a scaled tie, round away from zero
    exactly, rather than a host round() builtin.
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
            # remainder / denominator >= 1/2 - 2^-26
            rounded += remainder * 2**26 >= denominator * (2**25 - 1)
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
        and ("rule" not in want or got.get("rule") == want["rule"])
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


def report_types(path):
    if path.suffix == ".csv":
        columns = csv_cells(path)[0]
    else:
        import pyarrow.parquet as pq

        columns = pq.read_schema(path).names
    return {c: "int" if c in REPORT_INTS else "str" for c in columns}


def r_plain_table(table, expected):
    """Read R's plain doubles as the declared integers they hold exactly.

    The R package returns an `int` column as doubles, exact up to 2^53 in
    magnitude. A double that is not such a whole number keeps its encoding,
    so it cannot equal the independent decimal truth.
    """
    if table is None or expected is None:
        return table
    declared = {name: kind for name, kind in expected["columns"]}
    whole = [
        kind == "float" and declared.get(name) == "int"
        for name, kind in table["columns"]
    ]

    def cell(value, integral):
        if not integral or value is None:
            return value
        number = struct.unpack(">d", bytes.fromhex(value))[0]
        if math.isfinite(number) and number.is_integer():
            if abs(number) <= EXACT_DOUBLE:
                return str(int(number))
        return value

    return {
        "columns": [
            [name, "int" if integral else kind]
            for (name, kind), integral in zip(table["columns"], whole)
        ],
        "rows": [
            [cell(value, integral) for value, integral in zip(row, whole)]
            for row in table["rows"]
        ],
    }


def load_metamorphic_expectations(fixtures):
    """Reviewed empty-child/key-order differences that are known failures.

    Each entry names its tracking issue, or carries a by-design reason marked
    permanent. A non-permanent entry whose difference no longer occurs is
    stale and fails the run, so fixed issues prune their entries.
    """
    expectations = {}
    for case in benchmark_directories(fixtures):
        declared = load_metadata(case).get("conformance", {}).get("metamorphic", {})
        for check, value in declared.items():
            expectations[(case.name, check)] = {
                "benchmark": case.name,
                "check": check,
                **value,
            }
    return expectations


def unordered_report(tables):
    """The tables with the run report's rows as a sorted multiset.

    Check rows follow column declaration order (CHECK-23), so declaring the
    key columns in another order moves their rows; no row's facts change.
    """
    report = tables.get("report")
    if report is None:
        return tables
    rows = sorted(report["rows"], key=lambda row: json.dumps(row))
    return {**tables, "report": {"columns": report["columns"], "rows": rows}}


def build_difference(reference, variant, unordered_report_rows=False):
    """None when two builds are metamorphically equal, else the difference.

    A key-order variant compares its run report's rows without their order.
    """
    reference_issues, variant_issues = reference["issues"], variant["issues"]
    if not reference_issues and not variant_issues:
        tables = reference["tables"], variant["tables"]
        if unordered_report_rows:
            tables = tuple(map(unordered_report, tables))
        if tables[0] != tables[1]:
            return "typed tables differ"
        if reference.get("saved") != variant.get("saved"):
            return "saved outcome differs"
        return None
    if not reference_issues or not variant_issues:
        return "exactly one build reports issues"
    fields = ("phase", "condition", "rule", "spec_paths")
    if any(
        reference_issues[0].get(field) != variant_issues[0].get(field)
        for field in fields
    ):
        return {
            "first_issue": {
                "reference": reference_issues[0],
                "variant": variant_issues[0],
            }
        }
    return None


def judge_metamorphic(example, runtime, record, expectations):
    """Compare an example's variant builds against their reference builds.

    Records written before the metamorphic checks existed carry no
    "variants" list; those are reported as not judged rather than failed.
    """
    outcome = {
        "judged": False,
        "differences": [],
        "expected_failures": [],
        "stale": [],
        "problems": [],
    }
    observed = record.get("variants")
    if observed is None:
        return outcome
    outcome["judged"] = True
    planned = example.get("variants", {}).get(runtime) or []

    def identity(variant):
        return (
            variant.get("kind"),
            int(variant.get("entry")),
            variant.get("specification"),
        )

    if sorted(map(identity, observed)) != sorted(map(identity, planned)):
        outcome["problems"].append("observed variant builds do not match the plan")
        return outcome
    by_identity = {identity(variant): variant for variant in observed}
    builds = record.get("builds", [])
    groups = {}
    for variant in planned:
        reference = None
        if variant["kind"] == "key_order_permuted":
            control = next(
                candidate
                for candidate in planned
                if candidate["kind"] == "key_order_control"
                and candidate["entry"] == variant["entry"]
            )
            reference = by_identity[identity(control)]
        elif variant["entry"] < len(builds):
            reference = builds[variant["entry"]]
        if reference is None:
            outcome["problems"].append(
                f"missing reference build for variant {identity(variant)}"
            )
            continue
        check = (
            "key_order" if variant["kind"].startswith("key_order") else "empty_child"
        )
        detail = build_difference(
            reference,
            by_identity[identity(variant)],
            unordered_report_rows=check == "key_order",
        )
        if detail is not None:
            groups.setdefault(check, []).append(
                {"kind": variant["kind"], "entry": variant["entry"], "detail": detail}
            )
    for check, differences in groups.items():
        item = expectations.get((example["name"], check))
        if item is None:
            outcome["differences"].extend(
                {"check": check, **difference} for difference in differences
            )
        else:
            outcome["expected_failures"].append(
                {
                    "check": check,
                    "issue": item.get("issue"),
                    "reason": item.get("reason"),
                    "differences": differences,
                }
            )
    for (benchmark, check), item in expectations.items():
        if (
            benchmark == example["name"]
            and not item.get("permanent")
            and check not in groups
        ):
            outcome["stale"].append({"check": check, "issue": item.get("issue")})
    return outcome


def first_difference(first, second, path):
    """The path, first value and second value where two observations first
    differ, or None when they are equal."""
    if isinstance(first, dict) and isinstance(second, dict):
        for key in sorted(first.keys() | second.keys(), key=str):
            if key not in first or key not in second:
                return f"{path}.{key}", first.get(key), second.get(key)
            found = first_difference(first[key], second[key], f"{path}.{key}")
            if found is not None:
                return found
        return None
    if isinstance(first, list) and isinstance(second, list):
        for index, (left, right) in enumerate(zip(first, second)):
            found = first_difference(left, right, f"{path}[{index}]")
            if found is not None:
                return found
        if len(first) == len(second):
            return None
        index = min(len(first), len(second))
        return (
            f"{path}[{index}]",
            first[index] if index < len(first) else None,
            second[index] if index < len(second) else None,
        )
    return None if first == second else (path, first, second)


# The plan a second run of the same object must report, by the first run's:
# a compiled plan is kept and reused; a refused compilation keeps nothing, so
# the second run compiles again and is refused again.
REPEATED_PLAN = {"compiled": "reused", "refused": "refused"}


def judge_repeats(builds):
    """Each entry's object runs twice; the second run equals the first.

    A spec object's second run reuses the plan its first run compiled (#2821).
    It must say so in its private observations, reproduce the first run's
    issues, typed public tables and save outcome, and leave every saved file
    byte for byte as the first run saved it, so the saved bytes judged against
    the authored truth below are both runs' bytes. A finding names the first
    differing value or the changed files.
    """
    findings = []
    for build in builds:
        name = build["specification"]
        repeat = build.get("repeat")
        if not isinstance(repeat, dict):
            findings.append(f"{name} was not run a second time")
            continue
        expected = REPEATED_PLAN.get(build.get("plan"))
        if expected is None or repeat.get("plan") != expected:
            findings.append(
                {
                    "repeat": name,
                    "first_run_plan": build.get("plan"),
                    "second_run_plan": repeat.get("plan"),
                    "expected_second_run_plan": expected,
                }
            )
        for field in ("issues", "tables", "saved"):
            found = first_difference(build.get(field), repeat.get(field), field)
            if found is not None:
                at, first, second = found
                findings.append(
                    {
                        "repeat": name,
                        "differs_from_first_run": field,
                        "at": at,
                        "first_run": first,
                        "second_run": second,
                    }
                )
        if repeat.get("changed_files") != []:
            findings.append(
                {
                    "repeat": name,
                    "differs_from_first_run": "saved files",
                    "changed_files": repeat.get("changed_files"),
                }
            )
    return findings


def compare_example(example, runtime, record, fixtures, observed, expectations=None):
    if expectations is None:
        expectations = load_metamorphic_expectations(fixtures)
    result = {
        "example": example["name"],
        "verdict": "error",
        "unsupported_operations": [],
        "findings": [],
        "not_compared": [],
        "metamorphic": {
            "judged": False,
            "differences": [],
            "expected_failures": [],
            "stale": [],
            "problems": [],
        },
    }
    if example["excluded"]:
        result.update(verdict="excluded", reason=example["excluded"])
        return result
    case = fixtures / example["name"]
    if (case / "expected/spec_resolved.yaml").exists():
        result["not_compared"].append(
            {"artifact": "spec_resolved.yaml", "reason": RESOLVED_REASON}
        )
    if example.get("define"):
        for truth in sorted((case / "expected").iterdir()):
            if truth.suffix in {".xml", ".json", ".xpt"} or truth.name == "error.yaml":
                result["not_compared"].append(
                    {
                        "artifact": truth.name,
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
    repeats = judge_repeats(builds)
    result["findings"].extend(repeats)
    metamorphic = judge_metamorphic(example, runtime, record, expectations)
    result["metamorphic"] = metamorphic

    def finish():
        # A second run that differs from the first is a mismatch whatever the
        # first run reported.
        if repeats and result["verdict"] in ("pass", "unsupported"):
            result["verdict"] = "mismatch"
        if (
            metamorphic["problems"]
            or metamorphic["differences"]
            or metamorphic["stale"]
        ):
            result["findings"].append(
                {
                    "metamorphic": {
                        key: metamorphic[key]
                        for key in ("problems", "differences", "stale")
                    }
                }
            )
            if result["verdict"] == "pass":
                result["verdict"] = "mismatch"
        return result

    issues = [issue for build in builds for issue in build["issues"]]
    if any(i["phase"] == "engine" for i in issues):
        result["findings"].append({"engine_issues": issues})
        return finish()
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
            return finish()
        result.update(
            verdict="unsupported", unsupported_operations=sorted(set(operations))
        )
        return finish()
    negative = case / "expected/error.yaml"
    if negative.exists() and not example.get("define"):
        want = load_yaml(negative)
        if not issues or not diagnostic_matches(want, issues[0]):
            result["findings"].append(
                {
                    "diagnostic": "first issue differs",
                    "expected": want,
                    "actual": issues[0] if issues else None,
                }
            )
        if want.get("phase") == "validation":
            # expected/error.yaml is independent diagnostic truth. Keep the
            # complete-path controls here, against observations from actual
            # installed public hosts.
            paths = want["spec_paths"]

            def matches_path(actual, expected):
                return (
                    actual == expected
                    or actual.startswith(expected + ".")
                    or actual.startswith(expected + "[")
                )

            matched = set()
            for issue in issues:
                actual_paths = issue.get("spec_paths", [])
                owned = (
                    issue.get("phase") == "validation"
                    and issue.get("condition") == want["condition"]
                    and bool(actual_paths)
                    and all(
                        any(matches_path(actual, expected) for expected in paths)
                        for actual in actual_paths
                    )
                )
                if not owned:
                    # expected/error.yaml is a partial structured assertion.
                    # Independent same-phase findings may coexist (for
                    # example, a cycle and a separate forward reference).
                    # Complete-report truth owns exhaustive finding checks.
                    continue
                matched.update(
                    expected
                    for expected in paths
                    if any(matches_path(actual, expected) for actual in actual_paths)
                )
            missing = [path for path in paths if path not in matched]
            if missing:
                result["findings"].append({"missing_validation_paths": missing})
        if any(b["tables"].get("output") is not None or b.get("saved") for b in builds):
            result["findings"].append("negative build exposed or saved output")
        result["verdict"] = "mismatch" if result["findings"] else "pass"
        return finish()
    if issues:
        result.update(verdict="mismatch")
        result["findings"].append({"unexpected_issues": issues})
        return finish()
    compared = set()
    for entry, build in zip(entries, builds):
        types, output = declarations(case / entry["specification"])
        for slot, key in (("output", "path"), ("report", "report")):
            declared = output.get(key)
            actual = build["tables"].get(slot)
            if declared is None:
                # Every run that began holds its report; only a declared
                # report file has independent truth to compare.
                if slot == "output" and actual is not None:
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
            expected_types = types if slot == "output" else report_types(truth)
            expected = expected_table(truth, expected_types)
            if runtime == "r":
                actual = r_plain_table(actual, expected)
            if (
                slot == "output"
                and truth.suffix == ".csv"
                and output.get("decimals") is not None
            ):
                actual = reported_table(actual, output["decimals"])
                result["not_compared"].append(
                    {
                        "artifact": truth.name,
                        "reason": "unrounded float values: independent CSV supplies declared reported precision only (CSV-9)",
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
    return finish()


def compare(fixtures, observed, output):
    meta = read_json(observed / "run.json")
    if meta["protocol"] != PROTOCOL or meta["runtime"] not in {"python", "r"}:
        raise ValueError("unknown runner protocol/runtime")
    runtime = meta["runtime"]
    payload = plan(fixtures)
    authored = {entry["name"] for entry in payload["examples"]}
    unexpected = {p.stem for p in (observed / "records").glob("*.json")} - authored
    if unexpected:
        raise ValueError(
            "unexpected installed runner cases: " + ", ".join(sorted(unexpected))
        )
    expectations = load_metamorphic_expectations(fixtures)
    results = []
    for example in payload["examples"]:
        try:
            path = observed / "records" / (example["name"] + ".json")
            record = (
                read_json(path)
                if path.is_file()
                else {"error": "missing installed runner record"}
            )
            result = compare_example(
                example, runtime, record, fixtures, observed, expectations
            )
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
        "metamorphic": {
            "judged": sum(
                1 for r in results if (r.get("metamorphic") or {}).get("judged")
            ),
            "differences": sum(
                len((r.get("metamorphic") or {}).get("differences", []))
                for r in results
            ),
            "expected_failures": sum(
                len((r.get("metamorphic") or {}).get("expected_failures", []))
                for r in results
            ),
            "stale": [
                {"benchmark": r["example"], **stale}
                for r in results
                for stale in (r.get("metamorphic") or {}).get("stale", [])
            ],
        },
        "gate": {"mode": "all-active-must-pass", "failing": []},
    }
    # The gate is derived from the current directories and exclusions alone:
    # every active benchmark must pass on this lane. No prior observations
    # are consulted, so a new benchmark needs no registry edit to be gated.
    required = {entry["name"] for entry in payload["examples"] if not entry["excluded"]}
    passed = {r["example"] for r in results if r["verdict"] == "pass"}
    failing = sorted(required - passed)
    inventory["gate"] = {"mode": "all-active-must-pass", "failing": failing}
    status = int(bool(failing))
    write_json(output, inventory)
    print(
        json.dumps(
            {
                "runtime": runtime,
                **counts,
                "gate": inventory["gate"],
                "failures": [
                    {
                        "example": r["example"],
                        "verdict": r["verdict"],
                        "findings": r["findings"],
                    }
                    for r in results
                    if r["verdict"] not in ("pass", "excluded")
                ],
            },
            indent=2,
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
    complete = sub.add_parser("complete-reports")
    complete.add_argument("--fixtures", type=Path, required=True)
    complete.add_argument("--python-evidence", type=Path, required=True)
    complete.add_argument("--r-evidence", type=Path, required=True)
    complete.add_argument("--output", type=Path, required=True)
    complete.add_argument("--source-revision", required=True)
    args = parser.parse_args(argv)
    if args.command == "complete-reports":
        import subprocess

        from tools.conformance.complete_reports import compare_owners

        if args.fixtures.resolve() != (ROOT / "benchmarks").resolve():
            parser.error(
                "complete report truth requires the reviewed benchmark checkout"
            )
        if args.output.exists() or args.output.resolve().is_relative_to(
            args.fixtures.resolve()
        ):
            parser.error("output must be fresh and outside the authored fixture tree")
        actual = subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
        ).strip()
        if args.source_revision != actual:
            parser.error("candidate source revision differs from the reviewed checkout")
        result = compare_owners(
            args.fixtures, args.python_evidence, args.r_evidence, args.output, actual
        )
        print(json.dumps(result["summary"], indent=2))
        return 0
    if args.command == "plan":
        emit_plan(args.fixtures, args.output)
        return 0
    return compare(args.fixtures, args.observed, args.output)


if __name__ == "__main__":
    raise SystemExit(main())
