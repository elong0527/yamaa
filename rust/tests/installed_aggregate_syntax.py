"""Installed shared aggregate parser: independent truth and strict host boundaries."""

import csv
import json
import unittest
from contextlib import ExitStack, contextmanager
from pathlib import Path
from unittest.mock import patch

import installed_aggregate_planning as a
import installed_references as r
from yamaa import _native as yamaa_native
from yamaa.adapters import _native_dataset_plan
from yamaa.adapters._native_aggregate_syntax import (
    NativeAggregateLimitError,
    bind_aggregate_analyzer,
)
from yamaa.expressions.aggregate import (
    AggregateError,
    aggregate_identifiers,
    aggregate_star_datasets,
    parse_aggregate,
    ungrouped_identifiers,
)
from yamaa.planning import aggregate_syntax


@contextmanager
def forbid_reference_syntax():
    """Turn every retained reference syntax/metadata path into a visible test failure."""
    with ExitStack() as stack:
        for module, names in (
            (r.reference_planning, ("analyze_aggregate",)),
            (_native_dataset_plan, ("analyze_aggregate",)),
            (
                aggregate_syntax,
                (
                    "parse_aggregate_cached",
                    "aggregate_identifiers",
                    "aggregate_star_datasets",
                    "ungrouped_identifiers",
                ),
            ),
        ):
            for name in names:
                stack.enter_context(
                    patch.object(
                        module,
                        name,
                        side_effect=AssertionError("reference aggregate syntax"),
                    )
                )
        yield


def with_expression(text):
    """Replace one grouped expression without deriving expected diagnostics from either engine."""
    spec = a.grouped_count_spec()
    derivations = dict(spec.rows[0].derivations)
    derivations["PRESENT"] = r.expression({"aggregate": {"expr": text}})
    return spec.model_copy(
        update={"rows": [spec.rows[0].model_copy(update={"derivations": derivations})]}
    )


class AggregateSyntax(unittest.TestCase):
    """Exercise installed native artifacts without adding evaluator capabilities."""

    def test_independent_truth_and_reference(self):
        """Both parsers must match the written AST/diagnostics, not just each other."""
        path = Path(__file__).with_name("aggregate_syntax.tsv")
        with path.open(encoding="utf-8", newline="") as stream:
            for row in csv.DictReader(stream, delimiter="\t"):
                with self.subTest(row["id"]):
                    self.assertEqual(
                        yamaa_native.analyze_aggregate(row["request"]), row["expected"]
                    )
                    text = json.loads(row["request"])["expression"]
                    expected = json.loads(row["expected"])["outcome"]
                    try:
                        ast = parse_aggregate(text)
                    except AggregateError as error:
                        self.assertEqual(
                            expected,
                            {
                                "status": "invalid",
                                "condition": error.condition,
                                "requirement": error.requirement,
                                "position": {
                                    "byte": len(text[: error.position].encode()),
                                    "character": error.position,
                                },
                                "context": error.context,
                            },
                        )
                    else:
                        self.assertEqual(
                            expected,
                            {
                                "status": "parsed",
                                "ast": ast,
                                "identifiers": list(aggregate_identifiers(ast)),
                                "star_datasets": list(aggregate_star_datasets(ast)),
                                "ungrouped_identifiers": list(
                                    ungrouped_identifiers(ast)
                                ),
                            },
                        )

    def test_closed_transport_and_retry(self):
        """Invalid host/JSON input is not a grammar failure or cached result."""
        for request in [
            None,
            1,
            [],
            {},
            "{}",
            '{"protocol":"aggregate-syntax/1","expression":null}',
            '{"protocol":"aggregate-syntax/1","expression":"A","unknown":1}',
            '{"protocol":"aggregate-syntax/1","expression":"A","expression":"B"}',
            '{"protocol":"other","expression":"A"}',
        ]:
            with (
                self.subTest(request=request),
                self.assertRaises((TypeError, ValueError)),
            ):
                yamaa_native.analyze_aggregate(request)
        response = json.loads(
            yamaa_native.analyze_aggregate(
                '{"protocol":"aggregate-syntax/1","expression":"A"}'
            )
        )
        self.assertEqual(response["outcome"]["ungrouped_identifiers"], ["A"])

    def test_installed_type_information(self):
        """The source-rebuilt wheel must retain the same public typing surface."""
        package = Path(yamaa_native.__file__).parent
        self.assertTrue((package / "py.typed").is_file())
        stub = (package / "__init__.pyi").read_text(encoding="utf-8")
        self.assertIn("def analyze_aggregate(request: str) -> str:", stub)

    def test_limits_and_no_execution_claim(self):
        """Syntax resources stay separate and parsing does not activate an engine."""
        for text, resource in [
            ("A" * 65537, "bytes"),
            ("(" * 70 + "A" + ")" * 70, "depth"),
        ]:
            response = json.loads(
                yamaa_native.analyze_aggregate(
                    json.dumps({"protocol": "aggregate-syntax/1", "expression": text})
                )
            )
            self.assertEqual(response["outcome"]["status"], "resource_limit")
            self.assertEqual(response["outcome"]["resource"], resource)
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_admission_planning_lowering_capture_one_service(self):
        """Actual grouped execution consumes Rust ASTs through IO mutation and repeated use."""
        spec = a.grouped_count_spec()
        columns = [
            r.Column(name=name, type=kind)
            for name, kind in (
                ("K", "int"),
                ("TOTAL", "int"),
                ("AVERAGE", "float"),
                ("RECORDS", "int"),
                ("PRESENT", "int"),
            )
        ]
        expressions = {
            "TOTAL": "SUM(SRC.V)",
            "AVERAGE": "MEAN(SRC.V)",
            "RECORDS": "COUNT(SRC.*)",
            "PRESENT": "COUNT(SRC.V)",
        }
        spec = spec.model_copy(
            update={
                "columns": columns,
                "output": r.Output(path="out.csv", columns=[c.name for c in columns]),
                "rows": [
                    spec.rows[0].model_copy(
                        update={
                            "derivations": {
                                "K": r.expression({"source": "SRC.K"}),
                                **{
                                    name: r.expression({"aggregate": {"expr": text}})
                                    for name, text in expressions.items()
                                },
                            }
                        }
                    )
                ],
            }
        )
        sources = {
            "SRC": r.frame_from_values(
                (
                    r.TypedColumn(name="K", type="int"),
                    r.TypedColumn(name="V", type="int"),
                ),
                [[2, None], [1, 4], [2, 6], [1, 2]],
            )
        }
        expected = b"K,TOTAL,AVERAGE,RECORDS,PRESENT\n2,6,6,2,1\n1,6,3,2,2\n"
        reference = a.execute_with_source_provider(spec, lambda _: sources)
        self.assertEqual(r.render_artifact(reference.artifact), expected)
        invoke = yamaa_native.analyze_aggregate
        calls = []

        def analyze(request):
            """Observe requests to the real shared parser without manufacturing answers."""
            calls.append(json.loads(request)["expression"])
            return invoke(request)

        def provider(_):
            """An external host action must not swap a service already admitted for this run."""
            yamaa_native.analyze_aggregate = lambda _: self.fail("late service lookup")
            return sources

        with (
            forbid_reference_syntax(),
            patch.object(yamaa_native, "analyze_aggregate", analyze),
        ):
            run = r.native_datasets.execute_with_source_provider(spec, provider)
        self.assertEqual(run.result.status, "success")
        self.assertEqual(r.render_artifact(run.result.artifact), expected)
        self.assertCountEqual(calls, expressions.values())
        self.assertEqual(len(calls), 4)

    def test_grammar_failures_precede_activation_and_data(self):
        """Native admission retains independent grammar conditions, contexts and owning paths."""
        for text, condition, requirement, context in (
            (
                "SUM(COUNT(SRC.*))",
                "nested_reduction",
                "REQ-0502",
                {"outer": "SUM", "inner": "COUNT"},
            ),
            (
                "mystery(SRC.V)",
                "prohibited_function",
                "REQ-0500",
                {"function": "mystery"},
            ),
            (
                "SUM(SRC.V) > 1",
                "prohibited_construct",
                "REQ-0512",
                {"construct": "comparison"},
            ),
            ("\u2003SUM(", "invalid_aggregate_expression", "REQ-0499", {}),
        ):
            spec = with_expression(text)
            for project in (False, True):
                with (
                    self.subTest(text=text, project=project),
                    forbid_reference_syntax(),
                    patch.object(
                        r.native_datasets,
                        "activate_project",
                        side_effect=AssertionError("activation"),
                    ),
                ):
                    run = self.run_frontend(spec, project)
                self.assertEqual(run.result.status, "failure")
                self.assertEqual(
                    [
                        (d.condition, d.requirement, d.spec_paths, dict(d.context))
                        for d in run.result.diagnostics
                    ],
                    [
                        (
                            condition,
                            requirement,
                            ("rows[0].derivations.PRESENT.aggregate",),
                            {"expr": text, **context},
                        )
                    ],
                )
                self.assertEqual(run.result.handler_counts, ())
                self.assertEqual(run.verifications, ())

    def run_frontend(self, spec, project):
        """Both production entrypoints must stop before the forbidden source provider."""
        if project:
            return r.native_datasets.execute_with_project_functions(
                spec,
                lambda _: self.fail("source"),
                r.ROOT / "specification-functions" / "python",
                r.SCHEMA,
            )
        return r.native_datasets.execute_with_source_provider(
            spec, lambda _: self.fail("source")
        )

    def test_missing_service_and_transport_fail_without_fallback(self):
        """An unavailable or broken native service cannot silently select Python parsing."""
        for project in (False, True):
            for invoke, error in (
                (None, TypeError),
                (
                    lambda _: (_ for _ in ()).throw(RuntimeError("transport")),
                    RuntimeError,
                ),
            ):
                with (
                    self.subTest(project=project, error=error),
                    forbid_reference_syntax(),
                    patch.object(
                        r.native_datasets,
                        "activate_project",
                        side_effect=AssertionError("activation"),
                    ),
                    patch.object(yamaa_native, "analyze_aggregate", invoke),
                    self.assertRaises(error),
                ):
                    self.run_frontend(a.grouped_count_spec(), project)

    def test_native_resource_policy_precedes_host_effects_and_retry(self):
        """Resource limits remain distinct from grammar failures and cannot poison later runs."""
        spec = with_expression("(" * 70 + "SUM(SRC.V)" + ")" * 70)
        for project in (False, True):
            with (
                forbid_reference_syntax(),
                patch.object(
                    r.native_datasets,
                    "activate_project",
                    side_effect=AssertionError("activation"),
                ),
                self.assertRaises(NativeAggregateLimitError) as failure,
            ):
                self.run_frontend(spec, project)
            self.assertEqual(failure.exception.resource, "depth")
            self.assertEqual(failure.exception.limit, 64)
            self.assertEqual(set(failure.exception.position), {"byte", "character"})
        with forbid_reference_syntax():
            result = r.native_datasets.execute_with_source_provider(
                a.grouped_count_spec(), lambda _: a.grouped_count_source()
            ).result
        self.assertEqual(
            r.render_artifact(result.artifact), b"K,RECORDS,PRESENT\n2,2,1\n1,1,0\n"
        )

    def test_wire_metadata_and_error_coordinates_reach_planner_port(self):
        """The host projects shared metadata directly and preserves Unicode scalar positions."""
        analyze = bind_aggregate_analyzer(yamaa_native)
        with forbid_reference_syntax():
            syntax = analyze("SRC.K + SUM(SRC.V) + COUNT(SRC.*)")
            self.assertEqual(syntax.identifiers, ("SRC.K", "SRC.V"))
            self.assertEqual(syntax.star_datasets, ("SRC",))
            self.assertEqual(syntax.ungrouped_identifiers, ("SRC.K",))
            with self.assertRaises(AggregateError) as error:
                analyze("\u2003SUM(")
        self.assertEqual(error.exception.position, 5)


if __name__ == "__main__":
    unittest.main()
