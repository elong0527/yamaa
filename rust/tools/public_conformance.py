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
import math
import re
import shutil
import struct
from pathlib import Path

PROTOCOL = "public-conformance/1"
METAMORPHIC_PROTOCOL = "public-conformance-metamorphic/1"
FIELD_REFERENCE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*$")
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


def benchmark_directories(fixtures):
    """Authored cases, independent of runtime labels or qualification results."""
    return [
        case
        for case in sorted(fixtures.iterdir())
        if case.is_dir() and list(case.glob("spec*.yaml"))
    ]


def plan(fixtures):
    examples = []
    for case in benchmark_directories(fixtures):
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
        example = {
            "name": case.name,
            "entries": entries,
            "excluded": EXCLUDED.get(case.name),
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

    Eligible documents have no `rows` and no `parents`, at least two keys,
    and every key column reading one input field or a literal, so key columns
    may be declared in any order (REQ-0060, REQ-0062, REQ-0220).
    """
    if not isinstance(document, dict) or "rows" in document or "parents" in document:
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
    an empty child (REQ-0627); positive entries may also get the key-order
    control and permutation.
    """
    descriptors = {"python": [], "r": []}
    contents = {}
    if example["excluded"]:
        return descriptors, contents
    case = fixtures / example["name"]
    negative = (case / "expected" / "error.yaml").exists()
    for runtime in ("python", "r"):
        for index, entry in enumerate(example["entries"][runtime]):
            spec = Path(entry["specification"])
            child = (spec.parent / f"__child_{spec.stem}.yaml").as_posix()
            descriptors[runtime].append(
                {"kind": "empty_child", "entry": index, "specification": child}
            )
            contents[child] = f'schema_version: "1.0"\nparents: {spec.name}\n'
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
    # Metamorphic variant specifications are generated, never authored: the
    # Python runner copies them from the plan tree, the R runner receives them
    # inside its staged inputs.
    for example in payload["examples"]:
        if example["excluded"]:
            continue
        _, contents = example_variants(fixtures, example)
        for relative, text in contents.items():
            for base in (output / "variants", output / "r-inputs"):
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


def load_metamorphic_expectations(fixtures):
    """Reviewed empty-child/key-order differences that are known failures.

    Each entry names its tracking issue, or carries a by-design reason marked
    permanent. A non-permanent entry whose difference no longer occurs is
    stale and fails the run, so fixed issues prune their entries.
    """
    path = (
        fixtures.parent
        / "rust"
        / "qualification"
        / "public-conformance-metamorphic.json"
    )
    if not path.is_file():
        return {}
    payload = read_json(path)
    if payload.get("protocol") != METAMORPHIC_PROTOCOL:
        raise ValueError("unknown metamorphic expectations protocol")
    expectations = {}
    for item in payload["expected_failures"]:
        key = (item["benchmark"], item["check"])
        if key in expectations:
            raise ValueError(f"duplicate metamorphic expectation: {key}")
        expectations[key] = item
    return expectations


def build_difference(reference, variant):
    """None when two builds are metamorphically equal, else the difference."""
    reference_issues, variant_issues = reference["issues"], variant["issues"]
    if not reference_issues and not variant_issues:
        if reference["tables"] != variant["tables"]:
            return "typed tables differ"
        if reference.get("saved") != variant.get("saved"):
            return "saved outcome differs"
        return None
    if not reference_issues or not variant_issues:
        return "exactly one build reports issues"
    fields = ("phase", "condition", "requirement", "spec_paths")
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
        detail = build_difference(reference, by_identity[identity(variant)])
        if detail is not None:
            check = (
                "key_order"
                if variant["kind"].startswith("key_order")
                else "empty_child"
            )
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
    metamorphic = judge_metamorphic(example, runtime, record, expectations)
    result["metamorphic"] = metamorphic

    def finish():
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
        return finish()
    if issues:
        result.update(verdict="mismatch")
        result["findings"].append({"unexpected_issues": issues})
        return finish()
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
    return finish()


def compare(fixtures, observed, output, ratchet=None):
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
        "ratchet": {"mode": "report-only"},
    }
    status = 0
    if ratchet:
        # The current directories/exclusions define both hosts' mandatory set.
        # Prior passing observations identify new passes; they are not a runtime registry.
        previous = read_json(ratchet)[runtime]
        if len(previous) != len(set(previous)):
            raise ValueError(f"duplicate observed {runtime} case")
        required = {
            entry["name"] for entry in payload["examples"] if not entry["excluded"]
        }
        passed = {r["example"] for r in results if r["verdict"] == "pass"}
        inventory["ratchet"] = {
            "mode": "gate",
            "regressions": sorted(required - passed),
            "new_passes": sorted(passed - set(previous)),
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
