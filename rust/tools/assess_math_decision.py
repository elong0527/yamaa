"""Retain exact decision evidence without changing a production policy or golden.

Run with a normally installed yamaa Python package. Historical hosted reports,
independent accuracy certificates, local native observations and frozen historical
artifact witnesses stay distinct. Completion never implies universal accuracy or
release qualification. The script refuses incomplete or inconsistent evidence.
"""

import argparse
import importlib.metadata
import json
import os
import platform
import shutil
import tempfile
from collections import Counter
from contextlib import contextmanager
from pathlib import Path

from assess_math import decode, difference, normalized_bits
from check_math_portability import compare, observations, validate_hosts
from math_certificates import certify
from math_corpus import validate_corpus
from math_decision_corpus import CORPUS, cases

FUNCTIONS = ("EXP", "LN", "POWER")
WORKSPACE = Path(__file__).resolve().parents[1]


def wire(value):
    """Encode exact result types/bits; diagnostic integers never narrow to float."""
    if value is None:
        return {"missing": None}
    if type(value) is bool:
        return {"bool": value}
    if type(value) is float:
        return {"float": normalized_bits(value)}
    if type(value) is int:
        return {"int": str(value)}
    if type(value) is str:
        return {"str": value}
    if isinstance(value, dict):
        return {key: wire(item) for key, item in value.items()}
    if isinstance(value, (tuple, list)):
        return [wire(item) for item in value]
    raise ValueError(f"unadmitted observed value {type(value).__name__}")


def sample_inventory(directory):
    """Validate nine original report sets and retain every known mismatch input."""
    reports = []
    identities = []
    metadata = []
    tables = []
    mismatching_keys = set()
    for path in sorted(directory.glob("*/math-assessment.json")):
        report = json.loads(path.read_text(encoding="utf-8"))
        table = observations(report)
        validate_corpus(table)
        failures = {}
        for failure in report["mismatches"]:
            key = failure["case"], failure["function"]
            if key in failures or key not in table:
                raise ValueError("unknown or duplicate retained historical mismatch")
            inputs, candidate = table[key]
            category, ulps = difference(candidate, failure["reference"])
            if (
                tuple(failure["inputs"]) != inputs
                or failure["candidate"] != candidate
                or category is None
                or (failure["category"], failure["ulp_distance"]) != (category, ulps)
            ):
                raise ValueError("inconsistent retained historical mismatch")
            failures[key] = failure["reference"]
            mismatching_keys.add(key)
        for function in FUNCTIONS:
            samples = sum(key[1] == function for key in table)
            failed = sum(key[1] == function for key in failures)
            count = report["counts"][function]
            if (samples, failed, samples - failed) != (
                count["samples"],
                count["mismatches"],
                count["exact"],
            ):
                raise ValueError("inconsistent retained historical counts")
        reports.append(report)
        identities.append(path.parent.name)
        tables.append((table, failures))
        metadata.append(
            {
                "identity": path.parent.name,
                "host": report["host"],
                "counts": report["counts"],
            }
        )
    validate_hosts(
        [
            (
                report["host"]["system"],
                report["host"]["machine"],
                ".".join(report["host"]["python"].split(".")[:2]),
            )
            for report in reports
        ]
    )
    count = compare(reports)
    result = []
    for key in sorted(mismatching_keys):
        inputs, candidate = tables[0][0][key]
        reference_groups = {}
        for identity, (table, failures) in zip(identities, tables, strict=True):
            reference = failures.get(key, table[key][1])
            reference_groups.setdefault(reference, []).append(identity)
        accuracy = certify(key[1], list(inputs), candidate)
        result.append(
            {
                "case": key[0],
                "function": key[1],
                "inputs": list(inputs),
                "candidate": candidate,
                "historical": [
                    {"result": bits, "reports": hosts}
                    for bits, hosts in sorted(reference_groups.items())
                ],
                "accuracy": accuracy,
            }
        )
    return {
        "observations_per_report": count,
        "reports": metadata,
        "mismatch_union": result,
    }


def numeric_fixture(case, target="float", handler=False):
    """Describe independent fixture inputs and lifecycle expectations."""
    operands = case["inputs"]
    bindings = []
    for index, operand in enumerate(operands):
        value = {"missing": None} if operand == "missing" else {"float": operand}
        if index == 1 and "original_integer_exponent" in case:
            value = {"int": case["original_integer_exponent"]}
        bindings.append({"name": "AB"[index], "value": value})
    request = {
        "expression": case["function"] + ("(A, B)" if len(operands) == 2 else "(A)"),
        "column_path": "columns.VALUE",
        "target": target,
        "bindings": bindings,
    }
    if handler:
        request["unconvertible"] = {"value": {"int": "9"}}
    return request


def public_numeric(request):
    """Observe an independent numeric fixture through the installed public domain."""
    import struct
    from tempfile import TemporaryDirectory

    import yamaa

    import yaml

    types = {"ID": "int"}
    values = ["1"]
    columns = [{"name": "ID", "type": "int", "derivation": "SRC.ID"}]
    for binding in request["bindings"]:
        name, value = binding["name"], binding["value"]
        kind = "int" if "int" in value else "float"
        types[name] = kind
        content = (
            value["int"]
            if kind == "int"
            else ""
            if "missing" in value
            else repr(decode(value["float"]))
        )
        values.append(content)
        columns.append({"name": name, "type": kind, "derivation": "SRC." + name})
    expression = {"compute": {"expr": request["expression"]}}
    if "unconvertible" in request:
        expression = {
            "value": expression,
            "unconvertible": int(request["unconvertible"]["value"]["int"]),
        }
    columns.append(
        {"name": "VALUE", "type": request["target"], "derivation": expression}
    )
    document = {
        "schema_version": "1.0",
        "domain": "MATH",
        "keys": ["ID"],
        "input": {"SRC": {"path": "input.csv", "types": types}},
        "columns": columns,
        "output": {"path": "output.csv", "columns": ["ID", "VALUE"]},
    }
    with TemporaryDirectory(prefix="yamaa-math-public-") as temporary:
        directory = Path(temporary)
        (directory / "spec.yaml").write_text(
            yaml.safe_dump(document, sort_keys=False), encoding="ascii"
        )
        (directory / "input.csv").write_text(
            ",".join(types) + "\n" + ",".join(values) + "\n", encoding="ascii"
        )
        result = yamaa.domain(directory / "spec.yaml")
        if not result.issues.is_empty():
            return {
                "outcome": {
                    "status": "failure",
                    "diagnostic": result.issues.row(0, named=True),
                },
                "handler_counts": [],
            }
        value = result.output["VALUE"][0]
        cell = (
            {"missing": None}
            if value is None
            else {"int": str(value)}
            if request["target"] == "int"
            else {"float": struct.pack(">d", value).hex()}
        )
        observations = result._native.observations()
        counts = json.loads(observations)["handler_counts"] if observations else []
        return {"outcome": {"status": "value", "value": cell}, "handler_counts": counts}


def historical(case, target="float", handler=False):
    """Read exact frozen lifecycle evidence; never execute the retiring evaluator."""
    report = json.loads(
        (WORKSPACE / "qualification/math-decision-v1.json").read_text(encoding="ascii")
    )
    if target == "float" and not handler:
        rows = report["boundaries"]
    elif target == "int" and handler:
        rows = report["integer_handlers"]
    else:
        raise ValueError("no frozen historical observation for requested lifecycle")
    matches = [row for row in rows if row["input"] == case]
    if len(matches) != 1:
        raise ValueError("historical input is absent or ambiguous")
    return matches[0]["historical"]


def outcome_bits(outcome):
    """Project numerical bits separately from domain/transport diagnostics."""
    if outcome["status"] == "value":
        value = outcome["value"]
        if "missing" in value:
            return "missing"
        return value["float"]
    return outcome["diagnostic"]["requirement"]


def boundaries():
    """Keep actual native and historical observations beside independent input truth."""

    results = []
    for case in cases():
        native = public_numeric(numeric_fixture(case))
        reference = historical(case)
        candidate = outcome_bits(native["outcome"])
        certificate = certify(
            case["function"],
            case["inputs"],
            candidate if not candidate.startswith("REQ-") else "missing",
        )
        if certificate["status"] == "language" and (
            candidate != certificate["language_expected"]
            or outcome_bits(reference["outcome"]) != candidate
        ):
            raise AssertionError(
                f"independently specified language truth failed at {case['id']}"
            )
        results.append(
            {
                "input": case,
                "native": native,
                "historical": reference,
                "accuracy": certificate,
            }
        )
    return results


@contextmanager
def temporary_workspace(prefix):
    """Restore the caller directory before releasing a temporary workspace."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory(prefix=prefix) as temporary:
        try:
            yield temporary
        finally:
            os.chdir(previous)


def downstream(values):
    """Feed observed policy outputs into real shared predicates/verification/CSV.

    This policy-isolation witness does not emulate old platform math in a public
    run. Both exact observed values become declared source data for the current
    shared engine, so the comparison isolates their dependent observable effects.
    """
    import yamaa

    result = []
    with temporary_workspace("yamaa-math-decision-") as temporary:
        os.chdir(temporary)
        for identity, observed in values:
            Path("input.csv").write_text(
                f"ID,VALUE\n1,{decode(observed)!r}\n", encoding="ascii"
            )
            specification = {
                "schema_version": "1.0",
                "domain": "TEST",
                "keys": ["ID"],
                "input": {
                    "SRC": {
                        "path": "input.csv",
                        "types": {"ID": "int", "VALUE": "float"},
                    }
                },
                "output": {
                    "path": "result.csv",
                    "columns": ["ID", "VALUE", "ABOVE", "ROUNDED"],
                },
                "columns": [
                    {"name": "ID", "type": "int", "derivation": "SRC.ID"},
                    {"name": "VALUE", "type": "float", "derivation": "SRC.VALUE"},
                    {
                        "name": "ABOVE",
                        "type": "int",
                        "derivation": {
                            "case": [
                                {
                                    "when": "VALUE > 2.718281828459045",
                                    "then": {"literal": 1},
                                },
                                {"otherwise": {"literal": 0}},
                            ]
                        },
                    },
                    {
                        "name": "ROUNDED",
                        "type": "float",
                        "derivation": {
                            "compute": {"expr": "ROUND_HALF_AWAY_FROM_ZERO(VALUE, 15)"}
                        },
                    },
                ],
            }
            Path("spec.yaml").write_text(json.dumps(specification), encoding="ascii")
            built = yamaa.domain("spec.yaml")
            if not built.issues.is_empty() or built.output is None:
                raise AssertionError(
                    f"downstream witness could not execute: {built.issues}"
                )
            rows = [
                {key: wire(value) for key, value in row.items()}
                for row in built.output.to_dicts()
            ]
            built.save()
            csv = Path("result.csv").read_bytes()
            specification["verifications"] = [
                {
                    "assert": {
                        "id": "historical-exp-one-limit",
                        "require": "VALUE <= 2.718281828459045",
                    }
                }
            ]
            Path("spec.yaml").write_text(json.dumps(specification), encoding="ascii")
            verified = yamaa.domain("spec.yaml")
            result.append(
                {
                    "policy_observation": identity,
                    "input_bits": observed,
                    "rows": rows,
                    "canonical_csv_hex": csv.hex(),
                    "verification_issues": verified.issues.to_dicts(),
                    "verification_succeeded": verified.issues.is_empty(),
                }
            )
    return result


def fixture_evidence():
    """Replay five unchanged original fixtures through installed public domain."""
    import yamaa

    import yaml

    names = (
        "adam-adeg-derived-intervals",
        "schema-numeric-functions",
        "negative-log-zero",
        "adam-adsl-bmi",
        "adam-advs-percentiles",
    )
    evidence = []
    with temporary_workspace("yamaa-math-fixtures-") as temporary:
        for name in names:
            target = Path(temporary) / name
            shutil.copytree(WORKSPACE.parent / "benchmarks" / name, target)
            if (target / "python/environment.yaml").exists():
                shutil.copyfile(
                    WORKSPACE.parent / "python/uv.lock", target / "python/uv.lock"
                )
            os.chdir(target)
            environment = (
                "python/environment.yaml"
                if Path("python/environment.yaml").exists()
                else None
            )
            built = yamaa.domain("spec.yaml", environment=environment)
            observed = {
                "name": name,
                "native_issues": built.issues.to_dicts(),
                "native_artifacts": [],
            }
            if name == "negative-log-zero":
                expected = yaml.safe_load(
                    Path("expected/error.yaml").read_text(encoding="utf-8")
                )
                actual = [
                    {**row, "context": json.loads(row["context"])}
                    for row in built.issues.to_dicts()
                ]
                if built.output is not None or actual != [expected]:
                    raise AssertionError("unchanged negative LN diagnostic differs")
                observed.update(expected_diagnostic=expected, exact_diagnostic=True)
            else:
                if not built.issues.is_empty() or built.output is None:
                    raise AssertionError(
                        f"affected original native fixture failed: {name}: {built.issues}"
                    )
                built.save()
                expected_files = sorted(Path("expected").glob("*.csv"))
                if not expected_files:
                    raise AssertionError(f"missing committed CSV truth: {name}")
                for expected in expected_files:
                    actual, truth = (
                        Path(expected.name).read_bytes(),
                        expected.read_bytes(),
                    )
                    if actual != truth:
                        raise AssertionError(
                            f"unchanged benchmark truth differs: {name}/{expected.name}"
                        )
                    observed["native_artifacts"].append(
                        {
                            "name": expected.name,
                            "expected_hex": truth.hex(),
                            "actual_hex": actual.hex(),
                            "exact": True,
                        }
                    )
            evidence.append(observed)
    return evidence


def summarize(report):
    """Report exact classes and exclusions without converting disagreement into a pass."""
    union = report["sample"]["mismatch_union"]
    summary = {"mismatch_union": dict(Counter(row["function"] for row in union))}
    classes = Counter()
    for row in union:
        for historical in row["historical"]:
            category, ulps = difference(row["candidate"], historical["result"])
            if category is not None:
                classes[f"{row['function']}:{category}:ulp_distance={ulps}"] += 1
    summary["historical_mismatch_classes"] = dict(classes)
    for name, evidence in (
        ("sample_accuracy", [row["accuracy"] for row in union]),
        ("boundary_accuracy", [row["accuracy"] for row in report["boundaries"]]),
    ):
        summary[name] = dict(
            Counter((row["status"] + ":" + row["comparison"]) for row in evidence)
        )
    summary["boundary_cases"] = len(report["boundaries"])
    return summary


def main():
    """Write reviewable observations; never generate benchmark expected files."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reports", type=Path, required=True)
    parser.add_argument("--hosted-run", required=True)
    parser.add_argument("--artifact-reference", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    archive = (WORKSPACE / "qualification/math-decision-v1.json").resolve()
    if output == archive:
        parser.error("--output must not overwrite qualification/math-decision-v1.json")
    sample = sample_inventory(args.reports)
    boundary = boundaries()
    exp = next(row for row in boundary if row["input"]["id"] == "exp-one")
    report = {
        "schema_version": 1,
        "corpus": CORPUS,
        "policy": "PortableLibmV1",
        "source_commit": args.source_commit,
        "artifact_reference": args.artifact_reference,
        "hosted_sample_run": args.hosted_run,
        "historical_lifecycle_source": "frozen qualification/math-decision-v1.json; not rerun",
        "historical_lifecycle_provenance": {
            key: json.loads(
                (WORKSPACE / "qualification/math-decision-v1.json").read_text(
                    encoding="ascii"
                )
            )[key]
            for key in ("source_commit", "artifact_reference", "hosted_sample_run")
        },
        "local_host": {
            "system": platform.system(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "yamaa": importlib.metadata.version("yamaa"),
        },
        "sample": sample,
        "boundaries": boundary,
        "exp_one_downstream": downstream(
            [
                ("historical", outcome_bits(exp["historical"]["outcome"])),
                ("candidate", outcome_bits(exp["native"]["outcome"])),
            ]
        ),
        "integer_handlers": [],
        "affected_fixtures": fixture_evidence(),
    }

    for identity in ("power-integer-admitted", "power-integer-refused"):
        case = next(case for case in cases() if case["id"] == identity)
        report["integer_handlers"].append(
            {
                "input": case,
                "native": public_numeric(numeric_fixture(case, "int", True)),
                "historical": historical(case, "int", True),
            }
        )
    report["summary"] = summarize(report)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="ascii")
    print(json.dumps(report["summary"], indent=2))
    print(f"Retained decision evidence: {output}")


if __name__ == "__main__":
    main()
