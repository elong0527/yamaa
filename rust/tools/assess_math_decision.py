"""Retain exact decision evidence without changing a production policy or golden.

Run with a normally installed yamaa Python package. Historical hosted reports,
independent accuracy certificates, local native observations and policy-isolation
artifact witnesses stay distinct. Completion never implies universal accuracy or
release qualification. The script refuses incomplete or inconsistent evidence.
"""

import argparse
import importlib.metadata
import json
import math
import os
import platform
import shutil
import tempfile
from collections import Counter
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


def native_request(case, target="float", handler=False):
    """Use the existing qualification transport, never a public policy selector."""
    operands = case["inputs"]
    bindings = []
    for index, operand in enumerate(operands):
        value = {"missing": None} if operand == "missing" else {"float": operand}
        if index == 1 and "original_integer_exponent" in case:
            value = {"int": case["original_integer_exponent"]}
        bindings.append({"name": "AB"[index], "value": value})
    request = {
        "protocol": "numeric/1",
        "math_policy": "portable_libm_v1",
        "expression": case["function"] + ("(A, B)" if len(operands) == 2 else "(A)"),
        "column_path": "columns.VALUE",
        "target": target,
        "bindings": bindings,
    }
    if handler:
        request["unconvertible"] = {"value": {"int": "9"}}
    return request


def historical(case, target="float", handler=False):
    """Observe the retained Python parser and actual completed-value lifecycle."""
    from yamaa.expressions import ExpressionDispatcher, MappingResolver
    from yamaa.models import MISSING
    from yamaa.planning import PlannedDerivation
    from yamaa.runtime.lifecycle import (
        HandlerCounter,
        LifecycleCondition,
        evaluate_derivation,
    )
    from yamaa.specification.models import Expression, HandledExpression

    request = native_request(case, target, handler)
    arguments = {"value": Expression(root={"compute": request["expression"]})}
    if handler:
        arguments["unconvertible"] = 9
    declaration = HandledExpression(**arguments)
    planned = PlannedDerivation(
        column="VALUE",
        path="columns.VALUE.derivation",
        expression_path="columns.VALUE.derivation.value",
        declaration=declaration,
        dependencies=(),
    )
    bindings = {}
    for entry in request["bindings"]:
        kind, value = next(iter(entry["value"].items()))
        bindings[entry["name"]] = (
            MISSING
            if kind == "missing"
            else int(value)
            if kind == "int"
            else decode(value)
        )
    counter = HandlerCounter()
    counter.register_derivation(planned)
    try:
        value = evaluate_derivation(
            planned,
            target,
            {},
            lambda outputs: MappingResolver(bindings),
            ExpressionDispatcher(),
            counter,
        )
        outcome = {
            "status": "value",
            "value": wire(None if value is MISSING else value),
        }
    except LifecycleCondition as failure:
        diagnostic = failure.diagnostic
        outcome = {
            "status": "failure",
            "diagnostic": {
                "phase": diagnostic.phase,
                "condition": diagnostic.condition,
                "requirement": diagnostic.requirement,
                "spec_paths": list(diagnostic.spec_paths),
                "context": wire(diagnostic.context),
            },
        }
    # The public counter snapshot retains declared zero counts and selected fires.
    counts = [
        {
            "spec_path": count.spec_path,
            "handler": count.handler,
            "count": str(count.count),
        }
        for count in counter.snapshot()
    ]
    return {"outcome": outcome, "handler_counts": counts}


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
    from yamaa import _native

    results = []
    for case in cases():
        native = json.loads(_native.evaluate_numeric(json.dumps(native_request(case))))
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


def downstream(values):
    """Feed observed policy outputs into real shared predicates/verification/CSV.

    This policy-isolation witness does not emulate old platform math in a public
    run. Both exact observed values become declared source data for the current
    shared engine, so the comparison isolates their dependent observable effects.
    """
    import yamaa

    result = []
    previous = Path.cwd()
    try:
        with tempfile.TemporaryDirectory(prefix="yamaa-math-decision-") as temporary:
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
                                "compute": {
                                    "expr": "ROUND_HALF_AWAY_FROM_ZERO(VALUE, 15)"
                                }
                            },
                        },
                    ],
                }
                Path("spec.yaml").write_text(
                    json.dumps(specification), encoding="ascii"
                )
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
                Path("spec.yaml").write_text(
                    json.dumps(specification), encoding="ascii"
                )
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
    finally:
        os.chdir(previous)
    return result


def fixture_evidence():
    """Replay unchanged affected goldens and expose the remaining frontend refusal."""
    import yamaa
    from yamaa import _native
    from yamaa._reference_domain import yamaa_domain

    import yaml

    names = (
        "adam-adeg-derived-intervals",
        "schema-numeric-functions",
        "negative-log-zero",
        "adam-adsl-bmi",
        "adam-advs-percentiles",
    )
    evidence = []
    previous = Path.cwd()
    originals = {"EXP": math.exp, "LN": math.log, "POWER": math.pow}

    def captured_calls(policy, calls):
        def implementation(function):
            def call(*operands):
                inputs = [normalized_bits(float(value)) for value in operands]
                if policy == "historical":
                    result = originals[function](*operands)
                else:
                    request = native_request({"function": function, "inputs": inputs})
                    observed = json.loads(_native.evaluate_numeric(json.dumps(request)))
                    value = observed["outcome"]["value"]
                    result = (
                        float("inf") if "missing" in value else decode(value["float"])
                    )
                calls.append(
                    {
                        "function": function,
                        "inputs": inputs,
                        "result": normalized_bits(result),
                    }
                )
                return result

            return call

        return {
            "exp": implementation("EXP"),
            "log": implementation("LN"),
            "pow": implementation("POWER"),
        }

    try:
        with tempfile.TemporaryDirectory(prefix="yamaa-math-fixtures-") as temporary:
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
                if built.output is not None:
                    built.save()
                    for expected in sorted(Path("expected").glob("*.csv")):
                        actual = Path(expected.name).read_bytes()
                        truth = expected.read_bytes()
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
                elif name == "negative-log-zero":
                    expected = yaml.safe_load(Path("expected/error.yaml").read_text())
                    rows = built.issues.to_dicts()
                    actual = [
                        {**row, "context": json.loads(row["context"])} for row in rows
                    ]
                    if actual != [expected]:
                        raise AssertionError("unchanged negative LN diagnostic differs")
                    observed["expected_diagnostic"] = expected
                    observed["exact_diagnostic"] = True
                elif name == "adam-adeg-derived-intervals":
                    rows = built.issues.to_dicts()
                    if (
                        len(rows) != 1
                        or rows[0]["condition"] != "unsupported_operation"
                        or rows[0]["spec_paths"]
                        != ["rows[1].derivations.QTVAL.aggregate.filter"]
                    ):
                        raise AssertionError(
                            "unexpected filtered-aggregate frontend outcome"
                        )
                    observed["qualification"] = (
                        "frontend-blocked; numerical isolation only"
                    )
                    observed["follow_up"] = (
                        "#1751 shared filtered aggregate, then #1742 full fixture qualification"
                    )
                    observed["policy_isolation"] = []
                    # A preparatory experiment through the explicit retained
                    # reference route. It supplies no native-frontend capability.
                    for policy in ("historical", "candidate"):
                        calls = []
                        replacements = captured_calls(policy, calls)
                        try:
                            for function, replacement in replacements.items():
                                setattr(math, function, replacement)
                            isolated = yamaa_domain(
                                "spec.yaml", schema_root=WORKSPACE.parent / "yaml"
                            )
                        finally:
                            math.exp, math.log, math.pow = (
                                originals["EXP"],
                                originals["LN"],
                                originals["POWER"],
                            )
                        if not isolated.issues.is_empty() or isolated.output is None:
                            raise AssertionError(
                                f"numerical isolation failed: {isolated.issues}"
                            )
                        isolated.save()
                        artifacts = []
                        for expected in sorted(Path("expected").glob("*.csv")):
                            actual = Path(expected.name).read_bytes()
                            truth = expected.read_bytes()
                            if actual != truth:
                                raise AssertionError(
                                    "filtered aggregate numerical isolation changed golden"
                                )
                            artifacts.append(
                                {
                                    "name": expected.name,
                                    "expected_hex": truth.hex(),
                                    "actual_hex": actual.hex(),
                                    "exact": True,
                                }
                            )
                        observed["policy_isolation"].append(
                            {"policy": policy, "calls": calls, "artifacts": artifacts}
                        )
                    left, right = observed["policy_isolation"]
                    if left["calls"] != right["calls"]:
                        raise AssertionError(
                            "filtered aggregate original math-call observations differ"
                        )
                else:
                    raise AssertionError(
                        f"affected original native fixture failed: {name}: {built.issues}"
                    )
                evidence.append(observed)
    finally:
        os.chdir(previous)
        math.exp, math.log, math.pow = (
            originals["EXP"],
            originals["LN"],
            originals["POWER"],
        )
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
    from yamaa import _native

    for identity in ("power-integer-admitted", "power-integer-refused"):
        case = next(case for case in cases() if case["id"] == identity)
        report["integer_handlers"].append(
            {
                "input": case,
                "native": json.loads(
                    _native.evaluate_numeric(
                        json.dumps(native_request(case, "int", True))
                    )
                ),
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
