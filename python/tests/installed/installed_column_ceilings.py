"""Installed width boundaries for compiler, file and linked-artifact admission
(#2185, #2288 item 2, #2299).

The shared compiler admits 1,024 declared columns, output columns and typed
source fields, and refuses 1,025. The boundaries, the refusals' issue rows and
the wide inherited cases are the `column-ceilings` family of the shared host
truth, which the R `column_ceilings` suite reads too.
"""

import json
import tempfile
import unittest
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
from host_fixtures import first_difference
from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "column-ceilings")
ADMITTED = TRUTH["admitted"]
REFUSED = TRUTH["refused"]
ISSUE_FIELDS = ("phase", "condition", "rule", "spec_paths", "context")


def issue_rows(rows):
    """The rows' authored fields, with each context decoded."""
    return [
        {
            **{field: row[field] for field in ISSUE_FIELDS},
            "context": json.loads(row["context"]),
        }
        for row in rows
    ]


class ColumnCeilings(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-column-ceilings-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def write_case(
        self,
        name,
        *,
        columns=1,
        fields=1,
        typed=False,
        source_format="csv",
        selected=None,
    ):
        root = self.root / name
        root.mkdir()
        names = ["ID", *[f"C{index:04d}" for index in range(1, columns)]]
        header = ["ID", *[f"F{index:04d}" for index in range(1, fields)]]
        values = {field: ["S-001" if field == "ID" else "x"] for field in header}
        path = f"input.{source_format}"
        if source_format == "parquet":
            pq.write_table(pa.table(values), root / path)
        else:
            (root / path).write_text(
                ",".join(header)
                + "\n"
                + ",".join(value[0] for value in values.values())
                + "\n",
                encoding="ascii",
            )
        # A dataset is written as its bare path unless it also types fields.
        source = path
        if typed:
            source = {"path": path, "types": {field: "str" for field in header}}
        # A section lists only specifications of its model (SUB-3), so each
        # case declares one for the project route.
        document = {
            "yamaa": "1.0",
            "type": "spec",
            "model": "ADaM",
            "domain": "TEST",
            "keys": ["ID"],
            "input": {"SRC": source},
            "columns": [
                {"name": "ID", "type": "str", "label": "ID", "derivation": "SRC.ID"},
                *[
                    {
                        "name": field,
                        "type": "str",
                        "label": field,
                        "derivation": "'x'",
                    }
                    for field in names[1:]
                ],
            ],
            "output": {
                "path": "out.csv",
                "columns": names if selected is None else selected,
            },
        }
        spec = root / "spec.yaml"
        spec.write_text(json.dumps(document), encoding="ascii")
        return spec

    @staticmethod
    def project_file(spec):
        """A project beside spec that lists it; it calls no project function,
        so it names no environment (D110)."""
        project = spec.with_name("project.yaml")
        project.write_text(
            f'yamaa: "1.0"\ntype: project\nadam: {{specs: [{spec.name}]}}\n',
            encoding="ascii",
        )
        return project

    @staticmethod
    def routes(spec, project=None):
        """Check and run one specification, alone or as a project names it."""
        specification = (
            yamaa.spec(spec) if project is None else yamaa.project(project).spec("TEST")
        )
        return specification.check(), specification.run()

    def refusals(self, spec, project=None):
        """The issue rows of checking and of running a specification without a name.

        A project selects only a specification it can read a `domain` from, so
        through a project the refusal comes from checking and running the
        project, and each finding there also names the listed file as `entry`
        and `source`, which are removed after they are compared.
        """
        if project is None:
            frames = [result.issues for result in self.routes(spec)]
        else:
            listed = yamaa.project(project)
            frames = [listed.check().issues, listed.run().issues]
        findings = []
        for frame in frames:
            rows = issue_rows(frame.to_pylist())
            if project is not None:
                identity = spec.resolve().as_posix()
                for row in rows:
                    self.assertEqual(
                        (row["context"].pop("entry"), row["context"].pop("source")),
                        (identity, identity),
                        row,
                    )
            findings.append(rows)
        return findings

    def assert_build(self, spec, width=1):
        self.assertEqual(yamaa.spec(spec).check().issues.num_rows, 0)
        built = yamaa.spec(spec).run()
        self.assertEqual(built.issues.num_rows, 0)
        # A row up to 1,024 cells wide is compared as a summary of its first
        # difference (#3882).
        self.assertIsNone(
            first_difference(
                built.output.to_polars().row(0), ("S-001", *["x"] * (width - 1))
            )
        )
        self.assertEqual(built.output.to_polars().shape, (1, width))

    def assert_refusal(self, result, compiler=False):
        rows = result.issues.to_pylist()
        self.assertTrue(rows)
        self.assertTrue(
            any(row["condition"] == "engine_rejected" for row in rows), rows
        )
        if compiler:
            self.assertTrue(
                any(
                    json.loads(row["context"])["code"] == "compiler_limit"
                    for row in rows
                ),
                rows,
            )
        if hasattr(result, "save"):
            self.assertIsNone(result.output)
            with self.assertRaises(yamaa.Error):
                result.save()

    def linked_consumer(self, producer):
        """A consumer that types the producer's stored artifact through its link."""
        document = json.loads(producer.read_text(encoding="ascii"))
        document["input"]["SRC"] = {"path": "out.csv", "schema": "spec.yaml"}
        document["columns"] = document["columns"][:1]
        document["output"] = {"path": "consumer.csv", "columns": ["ID"]}
        consumer = producer.with_name("consumer.yaml")
        consumer.write_text(json.dumps(document), encoding="ascii")
        return consumer

    def boundary(self, kind, width):
        """The specification of one boundary at one width, and the width it builds."""
        name = f"{kind}-{width}"
        if kind == "declared_columns":
            # The declared-column ceiling counts live columns after pruning: a
            # narrow output with one live column builds, so the ceiling is
            # exercised with a live 1,025-column output.
            selected = ["ID"] if width == ADMITTED else None
            return self.write_case(name, columns=width, selected=selected), 1
        if kind == "output_columns":
            if width == ADMITTED:
                return self.write_case(name, columns=width), width
            names = ["ID", *[f"C{index:04d}" for index in range(1, ADMITTED)]]
            # Keep declared columns in range: the extra projection must reach
            # its own count refusal before duplicate-output validation, not
            # the column cap.
            spec = self.write_case(name, columns=ADMITTED, selected=[*names, "ID"])
            return spec, None
        if kind == "typed_fields":
            return self.write_case(name, fields=width, typed=True), 1
        if kind in ("csv_fields", "parquet_fields"):
            source_format = kind.removesuffix("_fields")
            return self.write_case(name, fields=width, source_format=source_format), 1
        if kind == "producer_columns":
            producer = self.write_case(name, columns=width)
            consumer = self.linked_consumer(producer)
            if width == ADMITTED:
                # A link types the consumer before the producer's artifact
                # exists; callers build and save the producer before its
                # consumer reads that artifact.
                self.assertEqual(yamaa.spec(consumer).check().issues.num_rows, 0)
                yamaa.spec(producer).run().save()
            return consumer, 1
        if kind == "stored_columns":
            # The link's metadata stays admissible; only the stored artifact's
            # width changes. The direct Rust decoder control separately pins
            # the contract.
            producer = self.write_case(name, columns=ADMITTED)
            consumer = self.linked_consumer(producer)
            header = ["ID", *[f"C{index:04d}" for index in range(1, width)]]
            producer.with_name("out.csv").write_text(
                ",".join(header)
                + "\n"
                + ",".join(["S-001"] + ["x"] * (width - 1))
                + "\n",
                encoding="ascii",
            )
            self.assertEqual(yamaa.spec(consumer).check().issues.num_rows, 0)
            return consumer, 1
        raise AssertionError(f"unknown boundary kind: {kind}")

    def test_each_width_ceiling_admits_1024_and_refuses_1025(self):
        for case in TRUTH["boundaries"]:
            with self.subTest(case=f"{ADMITTED} {case['name']}"):
                spec, width = self.boundary(case["kind"], ADMITTED)
                self.assert_build(spec, width=width)
            with self.subTest(case=f"{REFUSED} {case['name']}"):
                spec, _ = self.boundary(case["kind"], REFUSED)
                checked, built = self.routes(spec)
                results = {"check": checked, "run": built}
                for operation in case["operations"]:
                    self.assert_refusal(
                        results[operation], compiler=case["compiler_limit"]
                    )

    def test_compiler_refusal_details_on_check_and_run(self):
        for case in TRUTH["compiler_limits"]:
            resource, actual = case["resource"], case["actual"]
            for project in (False, True):
                route = "project" if project else "specification"
                with self.subTest(case=f"{resource} on the {route} route"):
                    spec = self.write_case(
                        f"details-{resource}-{route}",
                        columns=actual if resource == "columns" else 1,
                        fields=actual if resource == "source_fields" else 1,
                        typed=resource == "source_fields",
                    )
                    document = json.loads(spec.read_text(encoding="ascii"))
                    if resource == "inputs":
                        document["input"] = {
                            f"S{index}": "input.csv" for index in range(actual)
                        }
                        # Every specification is pruned after composition
                        # (LAYER-11 and LAYER-12): keep every input live
                        # through a single row-phase default so the inputs
                        # capacity is still exercised instead of being pruned.
                        document["columns"][0]["derivation"] = {
                            "first_available": {
                                "sources": [f"S{index}.ID" for index in range(actual)]
                            }
                        }
                    if resource == "projected_columns":
                        document["output"]["columns"] = ["ID"] * actual
                    spec.write_text(json.dumps(document), encoding="ascii")
                    routed = self.project_file(spec) if project else None
                    for result in self.routes(spec, routed):
                        self.assertEqual(
                            issue_rows(result.issues.to_pylist()),
                            issue_rows([case["issue"]]),
                        )

    def test_fixed_refusal_and_retry_expose_the_remaining_error(self):
        truth = TRUTH["incomplete_checks"]
        # Every specification is pruned after composition: keep all 1,025
        # columns live so the columns capacity is still exercised instead of
        # being pruned away.
        spec = self.write_case("incomplete-checks", columns=truth["columns"])
        document = json.loads(spec.read_text())
        document["columns"][1].update(type="int", derivation=1)
        document["columns"][2]["derivation"] = {
            "str_case": {"source": "C0001", "to": "upper"}
        }
        # The same refusal through a project that lists the specification.
        project = self.project_file(spec)
        for route in (None, project):
            spec.write_text(json.dumps(document))
            for refused in self.routes(spec, route):
                self.assert_refusal(refused, compiler=True)
                rows = refused.issues.to_pylist()
                self.assertEqual(len(rows), 1)
                context = json.loads(rows[0]["context"])
                self.assertNotIn("incomplete_checks_from", context)
                self.assertEqual(
                    {
                        "rule": rows[0]["rule"],
                        "actual": context["actual"],
                        "limit": context["limit"],
                    },
                    truth["refusal"],
                )
            repaired = dict(document)
            repaired["columns"] = document["columns"][:-1]
            # Dropping the C1024 declaration must also drop it from the live
            # output list, otherwise the repaired specification would fail on
            # the dangling output reference instead of the remaining type error.
            repaired["output"] = {
                **document["output"],
                "columns": [
                    name
                    for name in document["output"]["columns"]
                    if name != document["columns"][-1]["name"]
                ],
            }
            spec.write_text(json.dumps(repaired))
            for result in self.routes(spec, route):
                rows = result.issues.to_pylist()
                self.assertEqual(
                    [
                        {"condition": row["condition"], "rule": row["rule"]}
                        for row in rows
                    ],
                    [truth["remaining"]],
                )
                self.assertNotIn(
                    "incomplete_checks_from", json.loads(rows[0]["context"])
                )
            self.assertFalse(spec.with_name("out.csv").exists())

    def test_empty_inheritance_preserves_wide_case_columns_and_saved_bytes(self):
        truth = TRUTH["layered"]
        for width in truth["widths"]:
            root = self.root / f"layered-{width}"
            root.mkdir()
            (root / "input.csv").write_text(truth["input_csv"], encoding="ascii")
            names = ["ID", *[truth["column_name"] % index for index in range(1, width)]]

            def document():
                """The wide specification, whose text constants are quoted
                (D18)."""

                def text(value):
                    return f"'{value}'"

                return {
                    "yamaa": "1.0",
                    "type": "spec",
                    "domain": "TEST",
                    "keys": ["ID"],
                    "input": {"SRC": "input.csv"},
                    "columns": [
                        {"name": "ID", "type": "str", "derivation": "SRC.ID"},
                        *[
                            {
                                "name": name,
                                "type": "str",
                                "derivation": {
                                    "case": [
                                        {
                                            "when": f"SRC.A = 'x' AND SRC.B = '{index % 3}'",
                                            "then": text("P"),
                                        },
                                        {
                                            "when": "SRC.A IS NULL",
                                            "then": text("Q"),
                                        },
                                        {"otherwise": "SRC.A"},
                                    ]
                                },
                            }
                            for index, name in enumerate(names[1:], 1)
                        ],
                    ],
                    "output": {"path": "out.csv", "columns": names},
                }

            (root / "parent.yaml").write_text(json.dumps(document()), encoding="ascii")
            (root / "child.yaml").write_text(
                'yamaa: "1.0"\ntype: spec\nparent: parent.yaml\n', encoding="ascii"
            )
            (root / "grandchild.yaml").write_text(
                'yamaa: "1.0"\ntype: spec\nparent: child.yaml\n', encoding="ascii"
            )
            expected = [
                (
                    row["ID"],
                    *[
                        row["cells_by_index_mod_3"][index % 3]
                        for index in range(1, width)
                    ],
                )
                for row in truth["rows"]
            ]
            expected_bytes = (
                ",".join(names)
                + "\n"
                + "".join(",".join(row) + "\n" for row in expected)
            ).encode("ascii")
            for file in truth["files"]:
                with self.subTest(case=f"{file} of width {width}"):
                    spec = root / file
                    self.assertEqual(yamaa.spec(spec).check().issues.num_rows, 0)
                    built = yamaa.spec(spec).run()
                    self.assertEqual(built.issues.num_rows, 0)
                    # Up to 1,024 names and rows of up to 1,024 cells are
                    # compared as summaries of their first difference (#3882).
                    self.assertIsNone(
                        first_difference(built.output.column_names, names)
                    )
                    self.assertIsNone(
                        first_difference(built.output.to_polars().rows(), expected)
                    )
                    built.save()
                    self.assertEqual((root / "out.csv").read_bytes(), expected_bytes)

    def test_inheritance_depth_admits_original_chain_and_parent_occurrences_keep_fixed_bound(
        self,
    ):
        depth = TRUTH["inheritance_depth"]["layers_between_entry_and_base"]
        occurrences = TRUTH["parent_occurrences"]
        bound = occurrences["layers_between_entry_and_base"]
        for project in (False, True):
            route = "project" if project else "specification"
            spec = self.write_case(f"inheritance-{route}")
            spec.with_name("base.yaml").write_text(spec.read_text())
            for index in range(depth):
                parent = f"layer-{index + 1}.yaml" if index < depth - 1 else "base.yaml"
                spec.with_name(f"layer-{index}.yaml").write_text(
                    f'yamaa: "1.0"\ntype: spec\nparent: {parent}\n', encoding="ascii"
                )
            spec.write_text('yamaa: "1.0"\ntype: spec\nparent: layer-0.yaml\n')
            routed = self.project_file(spec) if project else None
            output = spec.with_name("out.csv")
            with self.subTest(case=f"{depth} layers on the {route} route"):
                checked, built = self.routes(spec, routed)
                self.assertEqual(checked.issues.num_rows, 0)
                self.assertEqual(built.issues.num_rows, 0)
                self.assertEqual(
                    built.output.to_polars().to_dict(as_series=False), {"ID": ["S-001"]}
                )
                built.save()
                self.assertEqual(
                    output.read_bytes(),
                    TRUTH["inheritance_depth"]["output_csv"].encode("ascii"),
                )

            # Each layer names one parent, so only a chain of 8,193 distinct
            # layers reaches the occurrence bound; without the bound it would
            # reach the base.
            for index in range(1, bound + 1):
                spec.with_name(f"chain-{index}.yaml").write_text(
                    f'yamaa: "1.0"\ntype: spec\nparent: chain-{index + 1}.yaml\n',
                    encoding="ascii",
                )
            spec.with_name(f"chain-{bound + 1}.yaml").write_text(
                spec.with_name("base.yaml").read_text()
            )
            spec.write_text('yamaa: "1.0"\ntype: spec\nparent: chain-1.yaml\n')
            with self.subTest(case=f"a chain of {bound} layers on the {route} route"):
                if not project:
                    for result in self.routes(spec):
                        self.assert_refusal(result)
                for rows in self.refusals(spec, routed):
                    self.assertEqual(rows, issue_rows([occurrences["issue"]]))
                self.assertEqual(
                    output.read_bytes(),
                    TRUTH["inheritance_depth"]["output_csv"].encode("ascii"),
                )


if __name__ == "__main__":
    unittest.main()
