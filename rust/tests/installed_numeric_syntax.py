"""Installed shared numeric syntax, independent truth and native planner ownership."""

import csv
import json
import unittest
from contextlib import ExitStack, contextmanager
from pathlib import Path
from unittest.mock import patch

import installed_references as r
from yamaa import _native as yamaa_native
from yamaa.adapters import _native_dataset_plan
from yamaa.adapters._native_numeric_syntax import (
    NativeNumericLimitError,
    bind_numeric_analyzer,
)
from yamaa.expressions.numeric import NumericError, numeric_identifiers, parse_numeric
from yamaa.planning import numeric_syntax
from yamaa.runtime.executor import execute_with_source_provider


@contextmanager
def forbid_reference_syntax():
    """Make every reference syntax/metadata path fail during native qualification."""
    with ExitStack() as stack:
        for module, names in (
            (r.reference_planning, ("analyze_numeric",)),
            (_native_dataset_plan, ("analyze_numeric",)),
            (numeric_syntax, ("parse_numeric_cached", "numeric_identifiers")),
        ):
            for name in names:
                stack.enter_context(
                    patch.object(
                        module,
                        name,
                        side_effect=AssertionError("reference numeric syntax"),
                    )
                )
        yield


def specification(text="COALESCE(SRC.X, 0) * 2"):
    """One expression used twice; exact expected CSV is independently written below."""
    return r.Specification(
        schema_version="1.0",
        domain="OUT",
        keys=["K"],
        input={"SRC": r.DatasetSource(path="source.csv")},
        output=r.Output(path="out.csv", columns=["K", "V", "W"]),
        columns=[
            r.Column(name=name, type=kind)
            for name, kind in (("K", "int"), ("V", "int"), ("W", "float"))
        ],
        rows=[
            r.Row(
                id="compute",
                dataset="SRC",
                derivations={
                    "K": r.expression({"source": "SRC.K"}),
                    "V": r.expression({"compute": {"expr": text}}),
                    "W": r.expression({"compute": {"expr": text}}),
                },
            )
        ],
    )


def sources():
    """Supply typed cells without consulting parser or engine output."""
    return {
        "SRC": r.frame_from_values(
            (r.TypedColumn(name="K", type="int"), r.TypedColumn(name="X", type="int")),
            [[1, 2], [2, None], [3, -4]],
        )
    }


class NumericSyntax(unittest.TestCase):
    """Both package forms must use Rust syntax without changing default Python behavior."""

    def test_independent_truth_and_reference(self):
        """Both implementations match authored ASTs or complete structured diagnostics."""
        path = Path(__file__).with_name("numeric_syntax.tsv")
        analyze = bind_numeric_analyzer(yamaa_native)
        with path.open(encoding="utf-8", newline="") as stream:
            for row in csv.DictReader(stream, delimiter="\t"):
                with self.subTest(row["id"]):
                    self.assertEqual(
                        yamaa_native.analyze_numeric(row["request"]), row["expected"]
                    )
                    text = json.loads(row["request"])["expression"]
                    expected = json.loads(row["expected"])["outcome"]
                    try:
                        ast = parse_numeric(text)
                    except NumericError as error:
                        self.assertEqual(
                            expected,
                            {
                                "status": "invalid",
                                "condition": error.condition,
                                "requirement": error.requirement,
                                "context": error.context,
                                "position": {
                                    "byte": len(text[: error.position].encode()),
                                    "character": error.position,
                                },
                            },
                        )
                        with (
                            forbid_reference_syntax(),
                            self.assertRaises(NumericError) as native,
                        ):
                            analyze(text)
                        self.assertEqual(
                            (
                                native.exception.condition,
                                native.exception.requirement,
                                native.exception.context,
                                native.exception.position,
                            ),
                            (
                                error.condition,
                                error.requirement,
                                error.context,
                                error.position,
                            ),
                        )
                    else:
                        self.assertEqual(
                            expected,
                            {
                                "status": "parsed",
                                "ast": ast,
                                "identifiers": list(numeric_identifiers(ast)),
                            },
                        )
                        with forbid_reference_syntax():
                            syntax = analyze(text)
                        self.assertEqual(syntax.ast, expected["ast"])
                        self.assertEqual(
                            syntax.identifiers, tuple(expected["identifiers"])
                        )

    def test_closed_transport_and_retry(self):
        """Host coercion and malformed metadata cannot become language outcomes or cached answers."""
        for request in [
            None,
            1,
            [],
            {},
            "{}",
            '{"protocol":"numeric-syntax/1","expression":null}',
            '{"protocol":"numeric-syntax/1","expression":"A","extra":1}',
            '{"protocol":"numeric-syntax/1","expression":"A","expression":"B"}',
            '{"protocol":"other","expression":"A"}',
        ]:
            with (
                self.subTest(request=request),
                self.assertRaises((TypeError, ValueError)),
            ):
                yamaa_native.analyze_numeric(request)
        actual = json.loads(
            yamaa_native.analyze_numeric(
                '{"protocol":"numeric-syntax/1","expression":"A"}'
            )
        )
        self.assertEqual(actual["outcome"]["identifiers"], ["A"])

    def test_installed_type_information(self):
        """The public service must be present in direct and source-rebuilt wheel typing."""
        package = Path(yamaa_native.__file__).parent
        self.assertTrue((package / "py.typed").is_file())
        self.assertIn(
            "def analyze_numeric(request: str) -> str:",
            (package / "__init__.pyi").read_text(encoding="utf-8"),
        )

    def test_admission_planning_lowering_capture_one_service(self):
        """Authored values and types survive planning with reference parsing forbidden."""
        spec = specification()
        expected = b"K,V,W\n1,4,4\n2,0,0\n3,-8,-8\n"
        reference = execute_with_source_provider(spec, lambda _: sources())
        self.assertEqual(reference.status, "success")
        self.assertEqual(r.render_artifact(reference.artifact), expected)
        invoke = yamaa_native.analyze_numeric
        calls = []

        def analyze(request):
            """Record calls to the actual shared parser without manufacturing its answers."""
            calls.append(json.loads(request)["expression"])
            return invoke(request)

        def provider(_):
            """Mutation after admission must not replace the captured service."""
            yamaa_native.analyze_numeric = lambda _: self.fail("late native lookup")
            return sources()

        with (
            forbid_reference_syntax(),
            patch.object(yamaa_native, "analyze_numeric", analyze),
        ):
            result = r.native_datasets.execute_with_source_provider(
                spec, provider
            ).result
        self.assertEqual(result.status, "success")
        self.assertEqual(r.render_artifact(result.artifact), expected)
        self.assertEqual(calls, ["COALESCE(SRC.X, 0) * 2"])

    def run_frontend(self, spec, project):
        """Both entrypoints must stop before activation or the forbidden source provider."""
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

    def test_grammar_diagnostics_precede_host_effects(self):
        """Exact language ownership and both authored paths remain visible before host IO."""
        for text, condition, requirement, context in (
            ("SUM(SRC.X)", "prohibited_function", "REQ-0440", {"function": "SUM"}),
            (
                "ABS(SRC.X,1)",
                "prohibited_function",
                "REQ-0440",
                {"function": "ABS", "argument_count": 2},
            ),
            ("( ) >", "prohibited_construct", "REQ-0441", {"construct": "comparison"}),
            ("\u2003(", "invalid_numeric_expression", "REQ-0439", {}),
            (None, "invalid_numeric_expression", "REQ-0439", {}),
        ):
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
                    run = self.run_frontend(specification(text), project)
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
                            (f"rows[0].derivations.{name}.compute.expr",),
                            {"expr": text, **context},
                        )
                        for name in ("V", "W")
                    ],
                )
                self.assertEqual(run.result.handler_counts, ())
                self.assertEqual(run.verifications, ())

    def test_numeric_policy_stays_unsupported_before_host_effects(self):
        """Accepting function syntax does not enable an unqualified numerical policy."""
        for text, function in (
            ("EXP(SRC.X)", "EXP"),
            ("LN(SRC.X)", "LN"),
            ("POWER(SRC.X,2)", "POWER"),
        ):
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
                    result = self.run_frontend(specification(text), project).result
                self.assertEqual(result.status, "unsupported")
                self.assertEqual(
                    [(f.operation, f.spec_path) for f in result.features],
                    [
                        (
                            f"numeric_function_{function}",
                            f"rows[0].derivations.{name}.compute",
                        )
                        for name in ("V", "W")
                    ],
                )
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_missing_service_and_transport_never_fall_back(self):
        """A missing or broken compiler cannot select the reference implementation."""

        def fail(_):
            """Represent a transport failure before any source request."""
            raise RuntimeError("transport")

        for invoke, error in ((None, TypeError), (fail, RuntimeError)):
            for project in (False, True):
                with (
                    self.subTest(project=project, error=error),
                    forbid_reference_syntax(),
                    patch.object(
                        r.native_datasets,
                        "activate_project",
                        side_effect=AssertionError("activation"),
                    ),
                    patch.object(yamaa_native, "analyze_numeric", invoke),
                    self.assertRaises(error),
                ):
                    self.run_frontend(specification(), project)

    def test_resource_failures_and_retry(self):
        """Compiler policy remains distinct from diagnostics and does not poison the next run."""
        for text, resource in (
            ("A" * 65537, "bytes"),
            ("(" * 70 + "SRC.X" + ")" * 70, "depth"),
        ):
            for project in (False, True):
                with (
                    self.subTest(resource=resource, project=project),
                    forbid_reference_syntax(),
                    patch.object(
                        r.native_datasets,
                        "activate_project",
                        side_effect=AssertionError("activation"),
                    ),
                    self.assertRaises(NativeNumericLimitError) as error,
                ):
                    self.run_frontend(specification(text), project)
                self.assertEqual(error.exception.resource, resource)
                self.assertEqual(set(error.exception.position), {"byte", "character"})
        with forbid_reference_syntax():
            result = r.native_datasets.execute_with_source_provider(
                specification(), lambda _: sources()
            ).result
        self.assertEqual(
            r.render_artifact(result.artifact), b"K,V,W\n1,4,4\n2,0,0\n3,-8,-8\n"
        )


if __name__ == "__main__":
    unittest.main()
