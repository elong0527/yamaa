"""Prepare independent scalable public workloads before running any candidate.

The expected values and CSV bytes are authored here using standard-library
algorithms. This module imports no yamaa package and never executes a candidate.
Original benchmark fixtures remain unchanged. Capacity refusals are observations,
not permission to increase engine quotas.
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import math
import shutil
import struct
from pathlib import Path

FAMILIES = ("lookup", "aggregate", "window", "callback", "producer")
DEFAULT_ROWS = (1_000, 10_000, 100_000)
REPOSITORY = Path(__file__).resolve().parents[2]


def json_bytes(value):
    return (
        json.dumps(value, ensure_ascii=False, allow_nan=False, indent=2) + "\n"
    ).encode()


def csv_bytes(columns, rows):
    text = io.StringIO(newline="")
    writer = csv.writer(text, lineterminator="\n")
    writer.writerow(columns)
    writer.writerows(rows)
    return text.getvalue().encode("utf-8")


def typed(value, kind):
    if value is None:
        return {"type": "missing", "value": None}
    if kind == "float":
        value = struct.pack(">d", value).hex()
    elif kind == "int":
        value = str(value)
    return {"type": kind, "value": value}


def column(name, kind, derivation=None):
    result = {"name": name, "type": kind, "label": name}
    if derivation is not None:
        result["derivation"] = derivation
    return result


def specification(domain, source, columns, keys, output="out.csv"):
    return {
        "schema_version": "1.0",
        "domain": domain,
        "keys": keys,
        "input": source,
        "output": {"path": output, "columns": [c["name"] for c in columns]},
        "columns": columns,
    }


class Case:
    def __init__(self, directory, family, rows, dimensions):
        self.directory = directory
        directory.mkdir(parents=True)
        self.files = []
        self.record = {
            "name": directory.name,
            "family": family,
            "driving_rows": rows,
            "dimensions": dimensions,
            "entry": "spec.yaml",
            "environment": None,
            "source_files": [],
            "expected_artifacts": [],
            "expected_output": None,
            "expected_issues": [],
            "expected_logs": {"verification_log": None, "warning_log": None},
            "expected_callbacks": None,
            "expected_nodes": ["spec.yaml"],
        }

    def write(self, name, data):
        target = self.directory / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        self.files.append({"path": name, "bytes": len(data)})

    def document(self, name, value):
        # JSON is a YAML 1.2 document; quoting every key/value avoids a second
        # parser, implicit YAML typing or implementation-derived declarations.
        self.write(name, json_bytes(value))

    def source(self, name, columns, rows):
        self.write(name, csv_bytes(columns, rows))
        self.record["source_files"].append({"path": name, "row_count": len(rows)})

    def expected(self, artifact, columns, kinds, rows, serialized=None, root=True):
        cells = [[typed(v, k) for v, k in zip(row, kinds)] for row in rows]
        if serialized is None:
            serialized = [
                [number(v) if k == "float" else v for v, k in zip(row, kinds)]
                for row in rows
            ]
        name = "expected/" + artifact
        self.write(name, csv_bytes(columns, serialized))
        self.record["expected_artifacts"].append(
            {"path": artifact, "expected": name, "row_count": len(rows)}
        )
        if root:
            self.document(
                "expected/output.json",
                {
                    "columns": list(zip(columns, kinds)),
                    "row_count": len(rows),
                    "rows": cells,
                },
            )
            self.record["expected_output"] = "expected/output.json"

    def finish(self):
        self.record["files"] = self.files
        return self.record


def number(value):
    if value is None:
        return ""
    if not math.isfinite(value):
        raise ValueError("independent workloads admit finite binary64 values only")
    return str(int(value)) if value.is_integer() else repr(value)


def lookup(case, rows, multiplicity):
    base = [[f"K{i:06d}"] for i in range(rows)]
    donors = []
    expected = []
    for i, (key,) in enumerate(base):
        if i % 7 == 0:
            expected.append([key, None])
            continue
        # Equal order keys pin first stored donor, including non-ASCII text.
        for donor in range(multiplicity):
            donors.append([key, 1, f"é,{i}:{donor}"])
        expected.append([key, f"é,{i}:0"])
    case.source("base.csv", ["ID"], base)
    case.source("donor.csv", ["KEY", "ORDER", "TEXT"], donors)
    spec = specification(
        "LOOKUP",
        {"BASE": "base.csv", "DONOR": {"path": "donor.csv", "types": {"ORDER": "int"}}},
        [column("ID", "str", "BASE.ID"), column("TEXT", "str", "SELECTED.TEXT")],
        ["ID"],
    )
    spec["base"] = "BASE"
    spec["intermediates"] = [
        {
            "id": "SELECTED",
            "dataset": "DONOR",
            "key": {"KEY": "ID"},
            "order_by": ["DONOR.ORDER"],
            "keep": "first",
            "no_match": None,
        }
    ]
    case.document("spec.yaml", spec)
    case.expected("out.csv", ["ID", "TEXT"], ["str", "str"], expected)


def aggregate(case, rows, width):
    pattern = (
        float(2**53),
        1.0,
        -float(2**53),
        1.0,
        None,
        -1.0,
        0.25,
        -0.25,
        3.0,
        -3.0,
    )
    source, expected = [], []
    for start in range(0, rows, width):
        group = start // width
        values = []
        for offset in range(min(width, rows - start)):
            value = None if group % 7 == 0 else pattern[offset % len(pattern)]
            values.append(value)
            source.append([f"G{group:06d}", number(value)])
        present = [v for v in values if v is not None]
        # Explicit left fold, never math.fsum: order is part of this workload.
        total = None
        for value in present:
            total = value if total is None else total + value
        expected.append([f"G{group:06d}", total])
    case.source("source.csv", ["GROUP", "VALUE"], source)
    spec = specification(
        "AGGREGATE",
        {"SOURCE": {"path": "source.csv", "types": {"VALUE": "float"}}},
        [column("GROUP", "str", "SOURCE.GROUP"), column("TOTAL", "float")],
        ["GROUP"],
    )
    spec["rows"] = [
        {
            "id": "total",
            "group_by": ["SOURCE.GROUP"],
            "derivations": {"TOTAL": {"aggregate": {"expr": "SUM(SOURCE.VALUE)"}}},
        }
    ]
    case.document("spec.yaml", spec)
    case.expected("out.csv", ["GROUP", "TOTAL"], ["str", "float"], expected)


def window(case, rows, width):
    pattern = (None, 7.0, 7.0, None, 2.0, 1.0, None, None, 4.0, 4.0)
    source, expected = [], []
    for start in range(0, rows, width):
        group = start // width
        values = [pattern[i % len(pattern)] for i in range(min(width, rows - start))]
        distinct = sorted({v for v in values if v is not None}, reverse=True)
        ranks = {v: i + 1 for i, v in enumerate(distinct)}
        previous = None
        for seq, value in enumerate(values, 1):
            name = f"G{group:06d}"
            source.append([name, seq, number(value)])
            carried = previous if value is None else value
            rank = len(distinct) + 1 if value is None else ranks[value]
            expected.append(
                [
                    name,
                    seq,
                    value,
                    rank,
                    values[seq - 2] if seq > 1 else None,
                    previous,
                    carried,
                ]
            )
            if value is not None:
                previous = value
    columns = [
        column("GROUP", "str", "SOURCE.GROUP"),
        column("SEQ", "int", "SOURCE.SEQ"),
        column("VALUE", "float", "SOURCE.VALUE"),
        column("RANK", "int", {"rank": {"method": "dense", "window": "VALUE_ORDER"}}),
        column(
            "PREVIOUS",
            "float",
            {"row_value": {"source": "VALUE", "offset": -1, "window": "SEQ_ORDER"}},
        ),
        column(
            "LAST",
            "float",
            {"previous_non_missing": {"source": "VALUE", "window": "SEQ_ORDER"}},
        ),
        column("CARRY", "float", {"locf": {"source": "VALUE", "window": "SEQ_ORDER"}}),
    ]
    spec = specification(
        "WINDOW",
        {"SOURCE": {"path": "source.csv", "types": {"SEQ": "int", "VALUE": "float"}}},
        columns,
        ["GROUP", "SEQ"],
    )
    spec["base"] = "SOURCE"
    spec["windows"] = {
        "SEQ_ORDER": {"group_by": ["GROUP"], "order_by": ["SEQ"]},
        "VALUE_ORDER": {
            "group_by": ["GROUP"],
            "order_by": [{"variable": "VALUE", "direction": "desc", "nulls": "last"}],
        },
    }
    case.source("source.csv", ["GROUP", "SEQ", "VALUE"], source)
    case.document("spec.yaml", spec)
    case.expected(
        "out.csv", [c["name"] for c in columns], [c["type"] for c in columns], expected
    )


def environment(case, callback=False):
    for host, package, function, lock in (
        ("python", "yamaa-benchmarks", "yamaa_benchmarks.project_value", "uv.lock"),
        ("r", "yamaabenchmarks", "yamaabenchmarks::project_value", "renv.lock"),
    ):
        functions = {}
        if callback:
            functions["identity"] = {
                "function": function,
                "description": "Installed numeric identity.",
                "params": [{"name": "x", "type": "float"}],
                "returns": "float",
                "tests": [
                    {
                        "id": "normal",
                        "covers": ["normal", "numeric-comparison"],
                        "args": {"x": 7.25},
                        "result": 7.25,
                    },
                    {
                        "id": "boundary",
                        "covers": ["boundary"],
                        "args": {"x": 0.0},
                        "result": 0.0,
                    },
                    {
                        "id": "missing",
                        "covers": ["short-circuit-missing:x"],
                        "args": {"x": None},
                        "result": None,
                    },
                ],
            }
        case.document(
            f"{host}/environment.yaml",
            {
                "schema_version": "1.0",
                "language": host,
                "lock": lock,
                "functions": functions,
            },
        )
        # Retain the complete reviewed study lock; no fabricated partial lock or
        # running-environment version update is performed by this generator.
        source = REPOSITORY / (
            "python/uv.lock"
            if host == "python"
            else "benchmarks/schema-functions/r/renv.lock"
        )
        case.write(f"{host}/{lock}", source.read_bytes())
    case.record["environment"] = {
        host: f"{host}/environment.yaml" for host in ("python", "r")
    }


def callback(case, rows, width):
    del width
    values = [None if i % 7 == 0 else (i % 23 - 11) / 4.0 for i in range(rows)]
    source = [[f"K{i:06d}", number(value)] for i, value in enumerate(values)]
    spec = specification(
        "CALLBACK",
        {"SOURCE": {"path": "source.csv", "types": {"VALUE": "float"}}},
        [
            column("ID", "str", "SOURCE.ID"),
            column(
                "VALUE",
                "float",
                {"function": {"name": "identity", "args": {"x": "SOURCE.VALUE"}}},
            ),
        ],
        ["ID"],
    )
    case.source("source.csv", ["ID", "VALUE"], source)
    case.document("spec.yaml", spec)
    case.expected(
        "out.csv",
        ["ID", "VALUE"],
        ["str", "float"],
        [[f"K{i:06d}", value] for i, value in enumerate(values)],
    )
    environment(case, callback=True)
    case.record["expected_callbacks"] = {
        "lock": 1,
        "binding": 1,
        "test_invocations": 2,
        "derivation_invocations": sum(v is not None for v in values),
        "missing_short_circuits": sum(v is None for v in values),
    }
    case.record["callback_count_limit"] = (
        "These are independent expectations; actual calls must be observed through once-delegating host capabilities, never inferred from a report array."
    )


def producer(case, rows, depth):
    values = (0.125, 0.375, -0.125, -0.375, 0.0)
    rounded = (0.13, 0.38, -0.13, -0.38, 0.0)
    source = [[f"K{i:06d}", number(values[i % len(values)])] for i in range(rows)]
    case.source("seed.csv", ["ID", "VALUE"], source)
    expected_nodes = []
    for index in range(depth):
        name = f"producer-{index}.yaml"
        inputs = (
            {"SOURCE": {"path": "seed.csv", "types": {"VALUE": "float"}}}
            if index == 0
            else {
                "SOURCE": {
                    "path": f"producer-{index - 1}.csv",
                    "schema": f"producer-{index - 1}.yaml",
                }
            }
        )
        spec = specification(
            f"PRODUCER{index}",
            inputs,
            [
                column("ID", "str", "SOURCE.ID"),
                column("VALUE", "float", "SOURCE.VALUE"),
            ],
            ["ID"],
            f"producer-{index}.csv",
        )
        spec["base"] = "SOURCE"
        spec["output"]["decimals"] = 2
        case.document(name, spec)
        output = [[f"K{i:06d}", rounded[i % len(rounded)]] for i in range(rows)]
        case.expected(
            f"producer-{index}.csv",
            ["ID", "VALUE"],
            ["str", "float"],
            output,
            [[k, f"{v:.2f}"] for k, v in output],
            root=False,
        )
        expected_nodes.append(name)
    # Two leaves share the entire producer chain; the root uses both leaves.
    for branch in ("left", "right"):
        spec = specification(
            branch.upper(),
            {
                "SOURCE": {
                    "path": f"producer-{depth - 1}.csv",
                    "schema": f"producer-{depth - 1}.yaml",
                }
            },
            [
                column("ID", "str", "SOURCE.ID"),
                column("VALUE", "float", "SOURCE.VALUE"),
            ],
            ["ID"],
            f"{branch}.csv",
        )
        spec["base"] = "SOURCE"
        spec["output"]["decimals"] = 2
        case.document(branch + ".yaml", spec)
        case.expected(
            branch + ".csv",
            ["ID", "VALUE"],
            ["str", "float"],
            output,
            [[k, f"{v:.2f}"] for k, v in output],
            root=False,
        )
        expected_nodes.append(branch + ".yaml")
    columns = [
        column("ID", "str", "LEFT.ID"),
        column("LEFT_VALUE", "float", "LEFT.VALUE"),
        column("RIGHT_VALUE", "float", "RIGHT_SELECTED.VALUE"),
        column("VALUE", "float", {"compute": {"expr": "LEFT_VALUE + RIGHT_VALUE"}}),
    ]
    spec = specification(
        "ROOT",
        {
            "LEFT": {"path": "left.csv", "schema": "left.yaml"},
            "RIGHT": {"path": "right.csv", "schema": "right.yaml"},
        },
        columns,
        ["ID"],
    )
    spec["base"] = "LEFT"
    spec["intermediates"] = [
        {"id": "RIGHT_SELECTED", "dataset": "RIGHT", "no_match": None}
    ]
    spec["output"] = {"path": "out.csv", "columns": ["ID", "VALUE"], "decimals": 2}
    case.document("spec.yaml", spec)
    root = [[k, v + v] for k, v in output]
    case.expected(
        "out.csv",
        ["ID", "VALUE"],
        ["str", "float"],
        root,
        [[k, f"{v:.2f}"] for k, v in root],
    )
    case.record["expected_nodes"] = expected_nodes + ["spec.yaml"]
    case.record["expected_producer_executions"] = len(expected_nodes)
    environment(case)


BUILDERS = dict(zip(FAMILIES, (lookup, aggregate, window, callback, producer)))
DIMENSIONS = {
    "lookup": (1, 4),
    "aggregate": (10, 100),
    "window": (10, 100),
    "callback": (1,),
    "producer": (1, 3),
}


def prepare(destination, sizes=DEFAULT_ROWS, families=FAMILIES):
    if (
        not sizes
        or any(type(n) is not int or n < 1 for n in sizes)
        or len(set(sizes)) != len(sizes)
    ):
        raise ValueError("row sizes must be distinct positive integers")
    if (
        not families
        or any(f not in FAMILIES for f in families)
        or len(set(families)) != len(families)
    ):
        raise ValueError("families must be distinct supported workload names")
    destination = Path(destination).resolve()
    destination.mkdir(exist_ok=False, parents=True)
    manifest = {
        "protocol": "scalable-public-workloads/1",
        "scope_decision": "2026-10-11 maintainer approval under #1742",
        "sizes": list(sizes),
        "sampling": {"cold": 10, "warm": 30, "warmups": 3},
        "expected_truth": "Independent standard-library algorithms authored before candidate execution; original benchmark fixtures unchanged.",
        "cases": [],
        "unavailable": {
            "numeric_budgets": None,
            "phase_attribution": None,
            "actual_callback_counts": None,
        },
    }
    for family in families:
        for rows in sizes:
            for dimension in DIMENSIONS[family]:
                key = {
                    "lookup": "donors_per_match",
                    "aggregate": "rows_per_group",
                    "window": "rows_per_partition",
                    "callback": "calls_per_present_row",
                    "producer": "shared_chain_depth",
                }[family]
                case = Case(
                    destination / f"{family}-{rows}-{dimension}",
                    family,
                    rows,
                    {key: dimension},
                )
                BUILDERS[family](case, rows, dimension)
                manifest["cases"].append(case.finish())
    (destination / "manifest.json").write_bytes(json_bytes(manifest))
    shutil.copyfile(Path(__file__), destination / "independent-generator.py")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rows", nargs="+", type=int, default=list(DEFAULT_ROWS))
    parser.add_argument(
        "--families", nargs="+", choices=FAMILIES, default=list(FAMILIES)
    )
    args = parser.parse_args()
    result = prepare(args.output, tuple(args.rows), tuple(args.families))
    print(
        json.dumps(
            {
                "output": str(args.output),
                "cases": len(result["cases"]),
                "sampling": result["sampling"],
            }
        )
    )
