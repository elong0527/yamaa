"""Installed fieldless relation binding, key validation and actual dependency ordering."""

import csv
import json
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import installed_references as r
import yamaa_native
from yamaa.runtime.executor import execute_with_source_provider


def specification(key, expr="COUNT(OTHER.*)"):
    """Place the consumer before its match-value producer; expected truth is authored below."""
    return r.Specification(
        schema_version="1.0",
        domain="OUT",
        base="SRC",
        keys=["K"],
        input={
            "SRC": r.DatasetSource(path="src.csv"),
            "OTHER": r.DatasetSource(path="other.csv"),
        },
        output=r.Output(path="out.csv", columns=["K", "V"]),
        columns=[
            r.Column(
                name="V",
                type="int",
                derivation=r.expression({"aggregate": {"expr": expr, "key": key}}),
            ),
            r.Column(
                name="K", type="int", derivation=r.expression({"source": "SRC.N"})
            ),
        ],
    )


def other():
    """Two matching donor rows and one nonmatching donor, without computed expected output."""
    return r.frame_from_values((r.TypedColumn(name="N", type="int"),), [[1], [1], [2]])


def plan(spec, native):
    """Reuse installed qualification's actual native compiler with reference helpers forbidden."""
    return r.InstalledReferences().plan(spec, native, {"OTHER": other()})


def grouped_count_spec():
    """Construct actual grouped COUNT syntax with independent output expectations below."""
    return r.Specification(
        schema_version="1.0",
        domain="OUT",
        base="SRC",
        input={"SRC": r.DatasetSource(path="source.csv")},
        keys=["K"],
        output=r.Output(path="out.csv", columns=["K", "RECORDS", "PRESENT"]),
        columns=[
            r.Column(name=name, type="int") for name in ("K", "RECORDS", "PRESENT")
        ],
        rows=[
            r.Row(
                id="count",
                dataset="SRC",
                group_by=["SRC.K"],
                derivations={
                    "K": r.expression({"source": "SRC.K"}),
                    "RECORDS": r.expression({"aggregate": {"expr": "COUNT(SRC.*)"}}),
                    "PRESENT": r.expression({"aggregate": {"expr": "COUNT(SRC.V)"}}),
                },
            )
        ],
    )


def grouped_count_source(rows=None):
    """Empty text is a present value here; direct typed ingestion does not rewrite it."""
    return {
        "SRC": r.frame_from_values(
            (r.TypedColumn(name="K", type="int"), r.TypedColumn(name="V", type="str")),
            [[2, None], [1, None], [2, ""]] if rows is None else rows,
        )
    }


class InstalledAggregatePlanning(unittest.TestCase):
    """The native metadata service and retained default planner obey independent expectations."""

    def test_grouped_count_execution_against_authored_csv(self):
        """Actual native execution counts records and present text without reference evaluation."""
        spec = grouped_count_spec()
        expected = b"K,RECORDS,PRESENT\n2,2,1\n1,1,0\n"
        reference = execute_with_source_provider(spec, lambda _: grouped_count_source())
        self.assertEqual(reference.status, "success")
        self.assertEqual(r.render_artifact(reference.artifact), expected)
        with patch(
            "yamaa.runtime.executor.execute_with_source_provider",
            side_effect=AssertionError("reference execution"),
        ):
            result = r.native_datasets.execute_with_source_provider(
                spec, lambda _: grouped_count_source()
            ).result
        self.assertEqual(result.status, "success")
        self.assertEqual(r.render_artifact(result.artifact), expected)

    def test_grouped_count_empty_and_filtered_output(self):
        """An empty input makes no groups; a row filter observes completed count results."""
        spec = grouped_count_spec()
        filtered = spec.model_copy(
            update={"rows": [spec.rows[0].model_copy(update={"filter": "RECORDS > 1"})]}
        )
        for case, rows, expected in [
            (spec, [], b"K,RECORDS,PRESENT\n"),
            (filtered, None, b"K,RECORDS,PRESENT\n2,2,1\n"),
        ]:
            for native in (False, True):
                with self.subTest(native=native, rows=rows):
                    provider = lambda _, rows=rows: grouped_count_source(rows)
                    result = (
                        r.native_datasets.execute_with_source_provider(
                            case, provider
                        ).result
                        if native
                        else execute_with_source_provider(case, provider)
                    )
                    self.assertEqual(result.status, "success")
                    self.assertEqual(r.render_artifact(result.artifact), expected)

    def test_grouped_count_capability_precedes_activation_and_data(self):
        """A preceding native package is refused by both frontends before host effects."""
        capabilities = json.loads(yamaa_native.dataset_capabilities())
        capabilities["features"].remove("grouped_count")
        with (
            patch.object(
                yamaa_native, "dataset_capabilities", lambda: json.dumps(capabilities)
            ),
            patch.object(
                r.native_datasets,
                "activate_project",
                side_effect=AssertionError("activation"),
            ),
        ):
            spec = grouped_count_spec()
            results = [
                r.native_datasets.execute_with_source_provider(
                    spec, lambda _: self.fail("source")
                ),
                r.native_datasets.execute_with_project_functions(
                    spec,
                    lambda _: self.fail("source"),
                    r.ROOT / "specification-functions" / "python",
                    r.SCHEMA,
                ),
            ]
        for run in results:
            self.assertEqual(run.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in run.result.features],
                [
                    ("native_grouped_count", f"rows[0].derivations.{name}.aggregate")
                    for name in ("RECORDS", "PRESENT")
                ],
            )
            self.assertEqual(run.result.handler_counts, ())
            self.assertEqual(run.verifications, ())

    def test_grouped_count_does_not_enable_aggregate_expression_or_filter(self):
        """Valid broader aggregate forms retain explicit whole-run unsupported admission."""
        for aggregate in [
            {"expr": "COUNT(SRC.V) + 1"},
            {"expr": "COUNT(SRC.V)", "filter": "SRC.V IS NOT NULL"},
        ]:
            spec = grouped_count_spec()
            derivations = dict(spec.rows[0].derivations)
            derivations["PRESENT"] = r.expression({"aggregate": aggregate})
            spec = spec.model_copy(
                update={
                    "rows": [
                        spec.rows[0].model_copy(update={"derivations": derivations})
                    ]
                }
            )
            result = r.native_datasets.execute_with_source_provider(
                spec, lambda _: self.fail("source")
            ).result
            self.assertEqual(result.status, "unsupported")
            self.assertIn(
                (
                    "aggregate_scope_or_expression",
                    "rows[0].derivations.PRESENT.aggregate",
                ),
                [(f.operation, f.spec_path) for f in result.features],
            )

    def test_key_findings_without_a_field_read(self):
        """The key contract does not depend on whether a reducer names a stored field."""
        cases = [
            (
                {"N": {"literal": True}},
                "COUNT(OTHER.*)",
                "incompatible_input_type",
                "REQ-0004",
                {"source": "key[N]", "expected": "bool", "actual": "int"},
            ),
            (
                {"N": {"literal": "x"}},
                "COUNT(OTHER.*)",
                "incompatible_input_type",
                "REQ-0004",
                {"source": "key[N]", "expected": "str", "actual": "int"},
            ),
            (
                {"N": {"literal": True}},
                "SUM(OTHER.N)",
                "incompatible_input_type",
                "REQ-0004",
                {"source": "key[N]", "expected": "bool", "actual": "int"},
            ),
            (
                {"ABSENT": "K"},
                "COUNT(OTHER.*)",
                "unknown_field",
                "REQ-0141",
                {"identifier": "OTHER.ABSENT"},
            ),
            (
                {"N": "ABSENT"},
                "COUNT(OTHER.*)",
                "unknown_field",
                "REQ-0141",
                {"identifier": "ABSENT"},
            ),
            (
                {"N": "K"},
                "COUNT(ABSENT.*)",
                "unknown_field",
                "REQ-0103",
                {"identifier": "ABSENT.*"},
            ),
        ]
        for key, expr, condition, requirement, context in cases:
            for native in (False, True):
                with (
                    self.subTest(key=key, expr=expr, native=native),
                    self.assertRaises(r.ExecutionPlanningError) as caught,
                ):
                    plan(specification(key, expr), native)
                self.assertEqual(
                    [
                        (d.condition, d.requirement, d.spec_paths, dict(d.context))
                        for d in caught.exception.diagnostics
                    ],
                    [
                        (
                            condition,
                            requirement,
                            ("columns.V.derivation.aggregate.expr",),
                            context,
                        )
                    ],
                )

    def test_dependencies_and_join_provenance(self):
        """Only real match-value reads order columns; aliases remain in join metadata."""
        for value, deps, match in [
            ("K", ("K",), "K"),
            ({"source": "K"}, ("K",), "key[N]"),
            ({"literal": 1}, (), "key[N]"),
            ({"literal": None}, (), "key[N]"),
            ({"greatest": {"sources": ["K", "K"]}}, ("K",), "key[N]"),
        ]:
            for expr in (
                "COUNT(OTHER.*)",
                "SUM(OTHER.N)",
                "COUNT(OTHER.*) + SUM(OTHER.N)",
            ):
                for native in (False, True):
                    with self.subTest(value=value, expr=expr, native=native):
                        result = plan(specification({"N": value}, expr), native)
                        self.assertEqual(
                            [column.column for column in result.columns],
                            ["K", "V"] if deps else ["V", "K"],
                        )
                        consumer = next(c for c in result.columns if c.column == "V")
                        self.assertEqual(consumer.dependencies, deps)
                        (join,) = result.resolved_joins
                        self.assertEqual(
                            (
                                join.dataset,
                                join.source,
                                join.key,
                                join.inferred,
                                join.spec_path,
                            ),
                            (
                                "OTHER",
                                (match,),
                                ("N",),
                                False,
                                "columns.V.derivation.aggregate.expr",
                            ),
                        )

    def test_inferred_count_keys_preserve_order_and_provenance(self):
        """An omitted key reaches shared inference and contributes a real graph edge."""
        spec = specification({"N": "K"})
        spec = spec.model_copy(
            update={
                "keys": ["N"],
                "output": r.Output(path="out.csv", columns=["N", "V"]),
                "columns": [
                    spec.columns[0].model_copy(
                        update={
                            "derivation": r.expression(
                                {"aggregate": {"expr": "COUNT(OTHER.*)"}}
                            )
                        }
                    ),
                    spec.columns[1].model_copy(update={"name": "N"}),
                ],
            }
        )
        for native in (False, True):
            result = plan(spec, native)
            self.assertEqual([c.column for c in result.columns], ["N", "V"])
            self.assertEqual(result.columns[1].dependencies, ("N",))
            (join,) = result.resolved_joins
            self.assertEqual(
                (join.source, join.key, join.inferred), (("N",), ("N",), True)
            )

    def test_reference_execution_and_native_scope_limit(self):
        """Actual execution counts the two matching records; metadata support enables no new executor scope."""
        for value in ("K", {"source": "K"}, {"literal": 1}):
            spec = specification({"N": value})

            def provider(_):
                """Provide independent records without delegating expected values to either engine."""
                return {
                    "SRC": r.frame_from_values(
                        (r.TypedColumn(name="N", type="int"),), [[1]]
                    ),
                    "OTHER": other(),
                }

            result = execute_with_source_provider(spec, provider)
            self.assertEqual(result.status, "success")
            self.assertEqual(r.render_artifact(result.artifact), b"K,V\n1,2\n")
            native = r.native_datasets.execute_with_source_provider(
                spec, lambda _: self.fail("source")
            )
            self.assertEqual(native.result.status, "unsupported")
            self.assertIn(
                ("aggregate_scope_or_expression", "columns.V.derivation.aggregate"),
                [(f.operation, f.spec_path) for f in native.result.features],
            )

    def test_grouped_count_reads_its_actual_relation(self):
        """A fieldless grouped reduction requires no local key or stored wildcard field."""
        spec = specification({"N": "K"}).model_copy(
            update={
                "columns": [
                    r.Column(name="K", type="int"),
                    r.Column(name="V", type="int"),
                ],
                "rows": [
                    r.Row(
                        id="group",
                        dataset="SRC",
                        group_by=["SRC.N"],
                        derivations={
                            "K": r.expression({"source": "SRC.N"}),
                            "V": r.expression({"aggregate": {"expr": "COUNT(SRC.*)"}}),
                        },
                    )
                ],
            }
        )
        for native in (False, True):
            result = plan(spec, native)
            self.assertEqual(result.columns, ())
            self.assertEqual([d.column for d in result.rows[0].derivations], ["K", "V"])
            self.assertEqual(result.resolved_joins, ())

    def test_relation_truth_owned_port_and_limits(self):
        """Authored catalog indices distinguish relations, fields, output names and empty schemas."""
        with (r.ROOT / "reference_relations.tsv").open(encoding="utf-8") as stream:
            cases = list(csv.DictReader(stream, delimiter="\t", quoting=csv.QUOTE_NONE))
        for case in cases:
            request = json.loads(case["request"])
            metadata = request["catalog"]
            columns = {f["name"]: f["type"] for f in metadata["outputs"]}
            bindings = r.reference_bindings.BindingPlan(
                domain="OUT",
                output_columns=tuple(columns),
                datasets={
                    d["name"]: r.reference_bindings.DatasetBinding(
                        dataset=d["name"],
                        columns=tuple(
                            r.TypedColumn(name=f["name"], type=f["type"])
                            for f in d["fields"]
                        ),
                    )
                    for d in metadata["datasets"]
                },
            )
            compiler = r.bind_reference_compiler(yamaa_native)(bindings, columns)
            expected = (
                json.loads(case["expected"])["outcome"]["results"][0]["dataset"]
                is not None
            )
            with self.subTest(case=case["case"]):
                self.assertEqual(
                    compiler.has_relation(request["queries"][0]["name"]), expected
                )
        with self.assertRaises(r.NativeReferenceLimitError) as caught:
            compiler.has_relation("\u00e9" * 32769)
        self.assertEqual(
            (
                caught.exception.resource,
                caught.exception.limit,
                caught.exception.required,
            ),
            ("reference_bytes", 65536, 65538),
        )
        self.assertTrue(compiler.has_relation("EMPTY"))

    def test_query_failure_has_no_python_fallback_and_allows_retry(self):
        """Planning must reach the actual new service; a failed query cannot become membership fallback."""
        original = yamaa_native._compile_reference_catalog
        failure = ValueError("relation query failed")

        def fail_relation(request):
            """Keep the actual catalog for every earlier query; fail only relation resolution."""
            catalog, status = original(request)

            def analyze(request):
                """Raise once the new query is reached, without substituting successful responses."""
                if any(
                    q["kind"] == "bind_relation" for q in json.loads(request)["queries"]
                ):
                    raise failure
                return catalog.analyze(request)

            return SimpleNamespace(analyze=analyze), status

        with (
            patch.object(yamaa_native, "_compile_reference_catalog", fail_relation),
            self.assertRaises(ValueError) as caught,
        ):
            plan(specification({"N": "K"}), True)
        self.assertIs(caught.exception, failure)
        self.assertEqual(
            [c.column for c in plan(specification({"N": "K"}), True).columns],
            ["K", "V"],
        )

    def test_previous_capability_refuses_before_activation_or_source(self):
        """Both frontends refuse the preceding query set before host side effects."""
        case = r.ROOT / "specification-functions"
        project = r.load_specification(case / "spec.yaml", r.SCHEMA).specification
        ordinary = r.load_specification(
            r.ROOT / "specification-adlb" / "spec.yaml", r.SCHEMA
        ).specification
        with (
            patch.object(
                yamaa_native,
                "reference_capabilities",
                lambda: (
                    '{"protocol":"reference-analysis/1","features":["binding","output_validation","qualified_validation","intermediate_validation","key_relations","match_value_typing"]}'
                ),
            ),
            patch.object(
                r.native_datasets,
                "activate_project",
                side_effect=AssertionError("activation"),
            ),
        ):
            outcomes = [
                r.native_datasets.execute_with_project_functions(
                    project, lambda _: self.fail("source"), case / "python", r.SCHEMA
                ),
                r.native_datasets.execute_with_source_provider(
                    ordinary, lambda _: self.fail("source")
                ),
            ]
        for run in outcomes:
            self.assertEqual(run.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in run.result.features],
                [("native_relation_binding", "$")],
            )
            self.assertEqual(run.result.handler_counts, ())
            self.assertEqual(run.verifications, ())


if __name__ == "__main__":
    unittest.main()
