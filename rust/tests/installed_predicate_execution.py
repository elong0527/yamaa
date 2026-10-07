"""Installed native predicate consumers, independent output truth and parser ownership."""

import csv
import json
import unittest
from contextlib import ExitStack, contextmanager
from pathlib import Path
from unittest.mock import patch

import installed_project_functions as project_functions
import installed_references as r
import yamaa_native
from yamaa.adapters import _native_dataset_plan, _native_predicate_plan
from yamaa.adapters._native_predicate_syntax import (
    NativePredicateLimitError,
    bind_predicate_analyzer,
)
from yamaa.expressions import PredicateError, predicates
from yamaa.planning import predicate_syntax


@contextmanager
def forbid_reference_predicates():
    """Fail on all reference parser/evaluator/regex entrypoints during native runs."""
    with ExitStack() as stack:
        for module, names in (
            (r.reference_planning, ("analyze_predicate",)),
            (_native_dataset_plan, ("analyze_predicate",)),
            (_native_predicate_plan, ("analyze_predicate",)),
            (predicate_syntax, ("parse_predicate",)),
            (
                predicates,
                (
                    "parse_predicate",
                    "parse_predicate_cached",
                    "_PredicateParser",
                    "compile_pattern",
                    "_evaluate",
                    "evaluate_predicate",
                ),
            ),
        ):
            for name in names:
                stack.enter_context(
                    patch.object(
                        module,
                        name,
                        side_effect=AssertionError("reference predicate execution"),
                    )
                )
        yield


def sources(rows=None):
    """Typed cells come from authored input, never a reference computation."""
    return {
        "SRC": r.frame_from_values(
            (r.TypedColumn(name="K", type="int"), r.TypedColumn(name="S", type="str")),
            [[1, "cat"], [2, None], [3, "bat"], [4, "dog"]] if rows is None else rows,
        )
    }


def specification(site="row", text=None):
    """Put predicates at distinct planning/lowering sites over the same source."""
    text = text or (
        "str_contains(S, 'a')"
        if site in {"row", "window", "checks"}
        else "str_contains(SRC.S, 'a')"
    )
    keyed = site in {"root", "source", "window", "checks"}
    columns = [
        r.Column(
            name=name,
            type=kind,
            **(
                {"derivation": r.expression({"source": f"SRC.{name}"})} if keyed else {}
            ),
        )
        for name, kind in (("K", "int"), ("S", "str"))
    ]
    arguments = (
        {"base": "SRC"}
        if keyed
        else {
            "rows": [
                r.Row(
                    id="records",
                    dataset="SRC",
                    derivations={
                        name: r.expression({"source": f"SRC.{name}"})
                        for name in ("K", "S")
                    },
                    filter=text,
                )
            ]
        }
    )
    if site == "root":
        arguments["filter"] = text
    elif site == "source":
        columns[1] = r.Column(
            name="S",
            type="str",
            derivation=r.expression({"source": {"variable": "SRC.S", "filter": text}}),
        )
    elif site == "window":
        columns.append(
            r.Column(
                name="N",
                type="int",
                derivation=r.expression(
                    {"row_number": {"window": {"order_by": ["K"], "filter": text}}}
                ),
            )
        )
    elif site == "checks":
        arguments["verifications"] = [
            r.Expression(
                root={"assert": {"require": "S IS NULL OR str_contains(S, 'a|dog')"}}
            ),
            r.Expression(root={"assert": {"when": text, "require": "K < 4"}}),
        ]
    return r.Specification(
        schema_version="1.0",
        domain="OUT",
        keys=["K"],
        input={"SRC": r.DatasetSource(path="source.csv")},
        output=r.Output(path="out.csv", columns=[c.name for c in columns]),
        columns=columns,
        **arguments,
    )


class PredicateExecution(unittest.TestCase):
    """Both package forms must preserve consumer semantics without host evaluation."""

    def test_shared_typed_consumer_truth(self):
        """R and Python independently replay the same authored transport observations."""
        root = Path(__file__).parent / "datasets"
        with (root / "predicate_execution.tsv").open(
            encoding="utf-8", newline=""
        ) as stream:
            for case in csv.DictReader(stream, delimiter="\t"):
                with self.subTest(case=case["case"]), forbid_reference_predicates():
                    source = (root / case["input"]).read_bytes()
                    actual, outcome = (
                        yamaa_native.execute_dataset(case["request"], source)
                        if case["secondary"] == "-"
                        else yamaa_native.execute_dataset_sources(
                            case["request"],
                            source,
                            [(root / case["secondary"]).read_bytes()],
                        )
                    )
                self.assertEqual(outcome, case["expected"])
                if case["snapshot"] == "null":
                    self.assertIsNone(actual)
                else:
                    self.assertEqual(
                        yamaa_native.table_snapshot(actual), case["snapshot"]
                    )

    def test_consumer_csv_truth_without_reference_parser(self):
        """Rows, root, source, windows and checks have separately authored exact CSV."""
        for site, expected in [
            ("row", b"K,S\n1,cat\n3,bat\n"),
            ("root", b"K,S\n1,cat\n3,bat\n"),
            ("source", b"K,S\n1,cat\n2,\n3,bat\n4,\n"),
            ("window", b"K,S,N\n1,cat,1\n2,,\n3,bat,2\n4,dog,\n"),
            ("checks", b"K,S\n1,cat\n2,\n3,bat\n4,dog\n"),
        ]:
            with self.subTest(site=site), forbid_reference_predicates():
                run = r.native_datasets.execute_with_source_provider(
                    specification(site), lambda _: sources()
                )
            self.assertEqual(run.result.status, "success", run)
            self.assertEqual(r.render_artifact(run.result.artifact), expected)
            if site == "checks":
                self.assertEqual([v.failure for v in run.verifications], [None, None])
                self.assertEqual([v.evaluated_count for v in run.verifications], [4, 4])
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_matching_limits_accumulate_across_rows_and_retry_fresh(self):
        """Thousands of individually cheap misses must not reset the dataset budget."""
        spec = specification("row", "str_contains(S, 'z')")
        with (
            forbid_reference_predicates(),
            self.assertRaises(r.native_datasets.NativeDatasetLimitError) as failure,
        ):
            r.native_datasets.execute_with_source_provider(
                spec, lambda _: sources([[i, "a" * 100] for i in range(50000)])
            )
        self.assertIn(
            failure.exception.resource,
            {"predicate_regex_work", "predicate_regex_state_cells"},
        )
        with forbid_reference_predicates():
            retry = r.native_datasets.execute_with_source_provider(
                spec, lambda _: sources([[1, "a" * 100]])
            )
        self.assertEqual(retry.result.status, "success")
        self.assertEqual(r.render_artifact(retry.result.artifact), b"K,S\n")

    def test_intermediate_filter_uses_native_parser_and_selected_donor(self):
        """Named donor admission and lowering cannot fall back to Python syntax."""
        spec = specification("source")
        spec = spec.model_copy(
            update={
                "input": {
                    "SRC": r.DatasetSource(path="source.csv"),
                    "OTHER": r.DatasetSource(path="other.csv"),
                },
                "columns": [
                    spec.columns[0],
                    r.Column(
                        name="S",
                        type="str",
                        derivation=r.expression({"source": "DONOR.S"}),
                    ),
                ],
                "intermediates": [
                    r.Intermediate(
                        id="DONOR",
                        dataset="OTHER",
                        key={"K": "K"},
                        filter="str_contains(OTHER.S, 'a')",
                        order_by=[{"variable": "OTHER.K"}],
                        keep="last",
                        no_match=None,
                    )
                ],
            }
        )
        with forbid_reference_predicates():
            run = r.native_datasets.execute_with_source_provider(
                spec, lambda _: {**sources(), "OTHER": sources()["SRC"]}
            )
        self.assertEqual(run.result.status, "success", run)
        self.assertEqual(
            r.render_artifact(run.result.artifact), b"K,S\n1,cat\n2,\n3,bat\n4,\n"
        )

    def test_filters_preserve_activation_callback_order_and_fatal_failures(self):
        """Root selection precedes callbacks; record filters follow all row assignments."""
        fixture = project_functions.InstalledProjectFunctions()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        root = fixture.spec.model_copy(
            update={"filter": "str_contains(SOURCE.ID, '^R[24]$')"}
        )
        rows = fixture.spec.model_copy(
            update={
                "columns": [fixture.spec.columns[0]]
                + [
                    column.model_copy(update={"derivation": None})
                    for column in fixture.spec.columns[1:]
                ],
                "rows": [
                    r.Row(
                        id="records",
                        dataset="SOURCE",
                        derivations={
                            column.name: column.derivation
                            for column in fixture.spec.columns[1:]
                        },
                        filter="str_contains(SOURCE.ID, '^R2$')",
                    )
                ],
            }
        )
        root_calls = [
            project_functions.CALLS[offset + row]
            for offset in (0, 5, 10, 15)
            for row in (1, 3)
        ]
        row_calls = [
            project_functions.CALLS[offset + row]
            for row in range(5)
            for offset in (0, 5, 10, 15)
        ]
        expected_lines = (
            (project_functions.CASE / "expected" / "test.csv")
            .read_bytes()
            .splitlines(keepends=True)
        )
        for spec, calls, indexes in (
            (root, root_calls, (0, 2, 4)),
            (rows, row_calls, (0, 2)),
        ):
            fixture.events.clear()
            fixture.read = False
            with forbid_reference_predicates():
                run = fixture.execute(spec=spec)
            self.assertEqual(run.result.status, "success", run)
            self.assertEqual(
                r.render_artifact(run.result.artifact),
                b"".join(expected_lines[i] for i in indexes),
            )
            self.assertEqual(
                fixture.events, project_functions.VECTORS + ["source"] + calls
            )
        for spec, first_call, key in (
            (root, root_calls[0], "R2"),
            (rows, row_calls[0], None),
        ):
            fixture.events.clear()
            fixture.read = False
            fixture.mode = "raised"
            with forbid_reference_predicates():
                run = fixture.execute(spec=spec)
            self.assertEqual(run.result.status, "failure")
            self.assertEqual(
                run.result.diagnostics[0].condition, "function_call_failed"
            )
            # Row assignments fail before the later key column exists; no
            # partial identity may be fabricated for that failure.
            self.assertEqual(
                run.result.diagnostics[0].context.get("keys"),
                None if key is None else [{"ID": key}],
            )
            self.assertEqual(
                fixture.events, project_functions.VECTORS + ["source", first_call]
            )

    def test_syntax_reasons_and_limits_remain_distinct_before_effects(self):
        """Rust reasons survive formatting, and compiler policies never become grammar errors."""
        analyzer = bind_predicate_analyzer(yamaa_native)
        for text, expected in [
            ("K >", "expected operand at character 4"),
            ("K @", "unexpected character '@' at character 3"),
            ("K \u200b", "unexpected character '\\u200b' at character 3"),
        ]:
            with (
                self.subTest(text=text),
                forbid_reference_predicates(),
                self.assertRaises(PredicateError) as failure,
            ):
                analyzer(text)
            self.assertEqual(str(failure.exception), expected)
        with (
            forbid_reference_predicates(),
            self.assertRaises(NativePredicateLimitError) as refusal,
        ):
            r.native_datasets.execute_with_source_provider(
                specification("row", "str_contains(S, 'a{1000001}')"),
                lambda _: self.fail("source"),
            )
        self.assertEqual(
            (
                refusal.exception.phase,
                refusal.exception.resource,
                refusal.exception.limit,
            ),
            ("regex_compile", "repetition", 1000000),
        )
        with forbid_reference_predicates():
            self.assertEqual(analyzer("TRUE").ast, {"kind": "boolean", "value": True})
        invoke = yamaa_native.analyze_predicate

        def older(request):
            """Model an older installed diagnostic contract without changing the parser."""
            result = json.loads(invoke(request))
            result["outcome"].pop("message", None)
            return json.dumps(result)

        with (
            patch.object(yamaa_native, "analyze_predicate", older),
            self.assertRaisesRegex(ValueError, "lacks grammar message"),
        ):
            r.native_datasets.execute_with_source_provider(
                specification("row", "K >"), lambda _: self.fail("source")
            )

    def test_capture_survives_provider_mutation(self):
        """A provider cannot replace the syntax callable captured before data access."""
        invoke = yamaa_native.analyze_predicate
        calls = []
        text = "str_contains(S, 'a')"

        def analyze(request):
            """Observe the real service rather than manufacture an expected AST."""
            calls.append(json.loads(request)["expression"])
            return invoke(request)

        def provider(_):
            """Planning after admission must retain the captured service."""
            yamaa_native.analyze_predicate = lambda _: self.fail("late service capture")
            return sources()

        with (
            forbid_reference_predicates(),
            patch.object(yamaa_native, "analyze_predicate", analyze),
        ):
            run = r.native_datasets.execute_with_source_provider(
                specification("row", text), provider
            )
        self.assertEqual(run.result.status, "success")
        self.assertEqual(r.render_artifact(run.result.artifact), b"K,S\n1,cat\n3,bat\n")
        self.assertEqual(calls, [text])

    def test_invalid_literal_precedes_activation_and_empty_data(self):
        """Malformed regex is a REQ-1244 diagnostic before either host effect."""
        text = "str_contains(S, '[')"
        for project in (False, True):
            with (
                self.subTest(project=project),
                forbid_reference_predicates(),
                patch.object(
                    r.native_datasets,
                    "activate_project",
                    side_effect=AssertionError("activation"),
                ),
            ):
                spec = specification("row", text)
                provider = lambda _: self.fail("source provider must not run")
                if project:
                    run = r.native_datasets.execute_with_project_functions(
                        spec,
                        provider,
                        r.ROOT / "specification-functions" / "python",
                        r.SCHEMA,
                    )
                else:
                    run = r.native_datasets.execute_with_source_provider(spec, provider)
            self.assertEqual(run.result.status, "failure")
            self.assertTrue(run.result.diagnostics)
            for diagnostic in run.result.diagnostics:
                self.assertEqual(
                    (
                        diagnostic.condition,
                        diagnostic.requirement,
                        diagnostic.spec_paths,
                    ),
                    ("invalid_predicate", "REQ-1244", ("rows[0].filter",)),
                )
                self.assertEqual(diagnostic.context["position"], 16)

    def test_older_artifact_refuses_regex_at_exact_site(self):
        """Syntax support alone cannot authorize an older dataset execution service."""
        available = json.loads(yamaa_native.dataset_capabilities())
        available["features"].remove("predicate_regex")
        for site, path in [
            ("row", "rows[0].filter"),
            ("root", "filter"),
            ("source", "columns.S.derivation.source.filter"),
        ]:
            with (
                self.subTest(site=site),
                forbid_reference_predicates(),
                patch.object(
                    yamaa_native, "dataset_capabilities", lambda: json.dumps(available)
                ),
                patch.object(
                    r.native_datasets,
                    "activate_project",
                    side_effect=AssertionError("activation"),
                ),
            ):
                run = r.native_datasets.execute_with_source_provider(
                    specification(site), lambda _: self.fail("source provider")
                )
            self.assertEqual(run.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in run.result.features],
                [("native_predicate_regex", path)],
            )

    def test_nontext_is_eager_type_error_missing_is_unknown(self):
        """A decisive false cannot hide a type failure; NOT cannot turn missing true."""
        with forbid_reference_predicates():
            failed = r.native_datasets.execute_with_source_provider(
                specification("row", "FALSE AND str_contains(K, '1')"),
                lambda _: sources(),
            )
            missing = r.native_datasets.execute_with_source_provider(
                specification("row", "NOT str_contains(S, 'a')"), lambda _: sources()
            )
        self.assertEqual(failed.result.status, "failure")
        self.assertEqual(
            [
                (d.condition, d.requirement, d.spec_paths)
                for d in failed.result.diagnostics
            ],
            # The existing public row-filter wrapper intentionally omits the
            # primitive requirement; typed transport retains REQ-1244.
            [("incompatible_input_type", None, ("rows[0].filter",))],
        )
        self.assertEqual(missing.result.status, "success")
        self.assertEqual(r.render_artifact(missing.result.artifact), b"K,S\n4,dog\n")


if __name__ == "__main__":
    unittest.main()
