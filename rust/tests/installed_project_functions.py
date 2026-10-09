"""Installed native project activation and dataset execution over unchanged R018 truth."""

import json
import shutil
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import polars as pl
import yamaa
from yamaa import _native as yamaa_native
from yamaa.adapters.native_datasets import (
    NativeActivationCache,
    execute_with_project_functions,
    execute_with_source_provider,
)
from yamaa.functions.activation import ActivationCache
from yamaa.functions.artifact import LoadedArtifact
from yamaa.functions.execution import activate_project_functions
from yamaa.functions.execution import (
    execute_with_project_functions as reference_project,
)
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.models import TypedTable
from yamaa.specification import load_specification

import yaml

ROOT = Path(__file__).parent
CASE = ROOT / "specification-functions"
SCHEMA = ROOT / "specification-yaml"
# Independent authored callback truth: four short-circuit vectors never invoke.
VECTORS = [
    (10.0, 4.0, 2, 0.0, False),
    (0.0, 5.0, 2, 0.0, False),
    (7.0, 2.0, 2, 0.0, False),
    (9.0, 4.0, 2, None, False),
    (1.0, 2.0, 2, 0.0, True),
    (1.0, 2.0, 2, 0.0, False),
]
VALUES = [(10.0, 4.0), (1.0, 3.0), (7.0, 2.0), (0.0, 5.0), (9.0, 4.0)]
CALLS = (
    [(n, d, 2, 0.0, False) for n, d in VALUES] * 2
    + [(n, d, 1, 0.0, True) for n, d in VALUES]
    + [
        (n, d, 2, a, False)
        for (n, d), a in zip(VALUES, [None, 0.5, 0.25, None, -0.5], strict=True)
    ]
)


class InstalledProjectFunctions(unittest.TestCase):
    """Prove Rust owns invocation, with activation-before-data and no reference fallback."""

    def setUp(self):
        """Resolve the original schema-normalized benchmark without opening source data."""
        self.spec = load_specification(CASE / "spec.yaml", SCHEMA).specification
        self.events = []
        self.read = False
        self.mode = None
        original = LoadedArtifact.load

        def load(artifact, call):
            """Wrap the verified artifact target with a concrete signature and effect recorder."""
            target = original(artifact, call)

            def observed(
                numerator, denominator, decimals=2, adjust=None, as_percent=False
            ):
                """Record one actual host invocation and inject only the selected failure scenario."""
                self.events.append(
                    (numerator, denominator, decimals, adjust, as_percent)
                )
                if self.mode == "activation_bad" or (
                    self.read and self.mode == "bad_result"
                ):
                    return "wrong type"
                if self.read and self.mode == "raised":
                    raise ValueError("project callback failed")
                if self.mode == "control":
                    raise KeyboardInterrupt("project interrupted")
                return target(numerator, denominator, decimals, adjust, as_percent)

            return observed

        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.object(LoadedArtifact, "load", load))

    def provider(self, declarations):
        """Observe the source boundary and read the unchanged CSV through the host adapter."""
        self.events.append("source")
        self.read = True
        return load_source_tables(declarations, ProjectResources(CASE))

    def execute(self, *, spec=None, cache=None, provider=None, project=None):
        """Forbid reference evaluation throughout native activation and dataset execution."""
        with ExitStack() as stack:
            for target in (
                "yamaa.functions.invocation.BoundFunction.invoke",
                "yamaa.runtime.executor.execute_specification",
                "yamaa.runtime.lifecycle.ExpressionDispatcher.evaluate",
                "yamaa.runtime.lifecycle.convert_value",
            ):
                stack.enter_context(
                    patch(target, side_effect=AssertionError("reference fallback"))
                )
            return execute_with_project_functions(
                self.spec if spec is None else spec,
                self.provider if provider is None else provider,
                CASE / "python" if project is None else project,
                SCHEMA,
                cache=cache,
            )

    def assert_csv(self, run):
        """Use committed independent bytes, not a reference-generated replacement golden."""
        self.assertEqual(run.result.status, "success")
        self.assertEqual(
            render_artifact(run.result.artifact),
            (CASE / "expected/test.csv").read_bytes(),
        )

    def test_unchanged_csv_and_entire_callback_trace(self):
        """Compare committed CSV and every vector/data callback without reference evaluation."""
        self.assertNotIn("/python/src/", yamaa.__file__)
        self.assert_csv(self.execute())
        self.assertEqual(self.events, VECTORS + ["source"] + CALLS)

    def test_native_cache_only_skips_vectors(self):
        """Retain all data callbacks on cache hits and rerun vectors after explicit clearing."""
        cache = NativeActivationCache()
        for expected in (VECTORS + ["source"] + CALLS, ["source"] + CALLS):
            self.events.clear()
            self.assert_csv(self.execute(cache=cache))
            self.assertEqual(self.events, expected)
        cache.clear()
        self.events.clear()
        self.assert_csv(self.execute(cache=cache))
        self.assertEqual(self.events, VECTORS + ["source"] + CALLS)

    def test_graph_failure_after_activation_and_source_allows_explicit_retry(self):
        """Analysis failure cannot execute data or invalidate already qualified vectors."""
        cache = NativeActivationCache()
        failure = ValueError("dependency analysis boundary failure")
        with (
            patch.object(
                yamaa_native, "analyze_dependencies", side_effect=failure
            ) as analyze,
            patch.object(
                yamaa_native,
                "execute_dataset_functions",
                side_effect=AssertionError("dataset execution after graph failure"),
            ),
            self.assertRaises(ValueError) as caught,
        ):
            self.execute(cache=cache)
        self.assertIs(caught.exception, failure)
        self.assertEqual(analyze.call_count, 1)
        self.assertEqual(self.events, VECTORS + ["source"])

        # No automatic retry or provider rollback is promised. The caller starts
        # another attempt; its provider reopens the original inputs. Cached vector
        # qualification remains valid even though the earlier dataset never ran.
        self.events.clear()
        self.assert_csv(self.execute(cache=cache))
        self.assertEqual(self.events, ["source"] + CALLS)

    def test_reference_compiler_failure_preserves_activation_and_retry(self):
        """Compile/query failures cannot execute callbacks or erase passed vector authority."""
        original = yamaa_native._compile_reference_catalog
        for stage in ("compile", "query"):
            with self.subTest(stage=stage):
                self.events.clear()
                self.read = False
                cache = NativeActivationCache()
                failure = ValueError("reference compiler boundary failure")
                calls = []

                def reject_query(_, calls=calls, failure=failure):
                    """Inject one query boundary failure after genuine catalog compilation."""
                    calls.append("query")
                    raise failure

                def compile_catalog(request, calls=calls, stage=stage, failure=failure):
                    """Distinguish preparation failure from failure of a ready catalog query."""
                    calls.append("compile")
                    if stage == "compile":
                        raise failure
                    catalog, status = original(request)
                    self.assertIsNotNone(catalog)
                    return SimpleNamespace(analyze=reject_query), status

                with (
                    patch.object(
                        yamaa_native, "_compile_reference_catalog", compile_catalog
                    ),
                    patch.object(
                        yamaa_native,
                        "execute_dataset_functions",
                        side_effect=AssertionError("execution after compiler failure"),
                    ),
                    self.assertRaises(ValueError) as caught,
                ):
                    self.execute(cache=cache)
                self.assertIs(caught.exception, failure)
                self.assertEqual(
                    calls, ["compile"] if stage == "compile" else ["compile", "query"]
                )
                self.assertEqual(self.events, VECTORS + ["source"])
                self.events.clear()
                self.assert_csv(self.execute(cache=cache))
                self.assertEqual(self.events, ["source"] + CALLS)

    def test_reference_activation_cannot_qualify_native(self):
        """Reject reference cache authority and independently qualify the native invocation path."""
        cache = ActivationCache()
        activate_project_functions(self.spec, CASE / "python", SCHEMA, cache=cache)
        self.events.clear()
        with self.assertRaisesRegex(TypeError, "NativeActivationCache"):
            self.execute(cache=cache)
        self.assertEqual(self.events, [])
        self.assert_csv(self.execute(cache=NativeActivationCache()))
        self.assertEqual(self.events, VECTORS + ["source"] + CALLS)

    def test_failed_activation_does_not_read_or_cache(self):
        """Stop on a failed vector and require a complete activation on the next attempt."""
        cache = NativeActivationCache()
        self.mode = "activation_bad"
        failed = self.execute(cache=cache)
        self.assertEqual(failed.result.status, "failure")
        self.assertEqual(
            failed.result.diagnostics[0].condition, "function_conformance_failed"
        )
        self.assertEqual(self.events, VECTORS[:1])
        self.mode = None
        self.events.clear()
        self.assert_csv(self.execute(cache=cache))
        self.assertEqual(self.events, VECTORS + ["source"] + CALLS)

    def test_control_exception_escapes_without_cache_or_source(self):
        """Preserve original interruption and leave data access and activation success untouched."""
        cache = NativeActivationCache()
        self.mode = "control"
        with self.assertRaises(KeyboardInterrupt):
            self.execute(cache=cache)
        self.assertEqual(self.events, VECTORS[:1])
        self.mode = None
        self.events.clear()
        self.assert_csv(self.execute(cache=cache))
        self.assertEqual(self.events, VECTORS + ["source"] + CALLS)

    def test_missing_capability_never_resolves_or_reads(self):
        """Reject unsupported native installations before artifact callbacks or source access."""
        with patch.object(
            yamaa_native,
            "dataset_capabilities",
            return_value=json.dumps(
                {"protocol": "dataset/1", "features": ["key_grain"]}
            ),
        ):
            result = self.execute()
        self.assertEqual(result.result.status, "unsupported")
        self.assertEqual(self.events, [])

    def test_bad_call_precedes_activation_and_source(self):
        """Report contract mismatch before invoking project code or reading study data."""
        self.spec.columns[1].derivation.value.root["function"]["contract_version"] = (
            "9.0.0"
        )
        result = self.execute()
        self.assertEqual(
            result.result.diagnostics[0].condition, "function_contract_mismatch"
        )
        self.assertEqual(self.events, [])

    def test_ordinary_api_still_refuses_project_callbacks(self):
        """Require explicit project authority rather than silently enabling ordinary native calls."""
        result = execute_with_source_provider(self.spec, self.provider)
        self.assertEqual(result.result.status, "unsupported")
        self.assertEqual(self.events, [])

    def test_data_callback_failures_are_fatal_with_original_identity(self):
        """Preserve the first failing callback path and complete identity with reference diagnostics."""
        for mode, condition in (
            ("bad_result", "invalid_function_result"),
            ("raised", "function_call_failed"),
        ):
            with self.subTest(mode=mode):
                self.events.clear()
                self.read = False
                self.mode = mode
                result = self.execute()
                self.assertEqual(result.result.status, "failure")
                diagnostic = result.result.diagnostics[0]
                self.assertEqual(diagnostic.condition, condition)
                self.assertEqual(
                    diagnostic.spec_paths, ("columns.RATIO.derivation.function",)
                )
                self.assertEqual(diagnostic.context["keys"], [{"ID": "R1"}])
                self.assertEqual(self.events, VECTORS + ["source"] + CALLS[:1])
                self.read = False
                reference = reference_project(
                    self.spec, self.provider, CASE / "python", SCHEMA, cache=None
                )
                self.assertEqual(result.result.diagnostics, reference.diagnostics)

    def test_provider_cannot_replace_admitted_expression_or_binding(self):
        """Keep the admitted snapshot and captured target despite mutations during source IO."""

        def provider(declarations):
            """Supply controlled source data while preserving an observable source boundary."""
            self.spec.columns[1].derivation.value.root.clear()
            # A later module mutation cannot replace an already captured target.
            with patch.object(
                LoadedArtifact, "load", side_effect=AssertionError("late binding")
            ):
                return self.provider(declarations)

        self.assert_csv(self.execute(provider=provider))
        self.assertEqual(self.events, VECTORS + ["source"] + CALLS)

    def test_changed_vector_identity_invalidates_native_cache(self):
        """Requalify changed vector truth even when artifact and contract identities stay equal."""
        cache = NativeActivationCache()
        self.assert_csv(self.execute(cache=cache))
        self.events.clear()
        self.read = False
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory) / "project"
            shutil.copytree(CASE / "python", project)
            path = project / "conformance/project_ratio.yaml"
            vectors = yaml.safe_load(path.read_text())
            vectors["cases"][0]["result"] = 99.0
            path.write_text(yaml.safe_dump(vectors, sort_keys=False))
            result = self.execute(cache=cache, project=project)
        self.assertEqual(
            result.result.diagnostics[0].condition, "function_conformance_failed"
        )
        self.assertEqual(self.events, VECTORS[:1])

    def test_later_ambiguous_argument_precedes_missing_short_circuit(self):
        """Resolve later collected arguments before applying an earlier missing short circuit."""

        def provider(declarations):
            """Supply controlled source data while preserving an observable source boundary."""
            sources = self.provider(declarations)
            table = sources["SOURCE"].table
            # The first argument is missing on both feeders; the later argument
            # must still report its two distinct values before any data callback.
            first = table.frame.head(1).with_columns(
                pl.lit(None, dtype=pl.Float64).alias("NUM")
            )
            second = first.with_columns(pl.lit(5.0).alias("DEN"))
            return {
                "SOURCE": TypedTable(columns=table.columns, frame=first.vstack(second))
            }

        result = self.execute(provider=provider)
        diagnostic = result.result.diagnostics[0]
        self.assertEqual(diagnostic.condition, "multiple_values_per_key")
        self.assertEqual(
            diagnostic.context,
            {"identifier": "SOURCE.DEN", "value_count": 2, "keys": [{"ID": "R1"}]},
        )
        self.assertEqual(self.events, VECTORS + ["source"])

    def test_record_templates_preserve_row_major_callback_order(self):
        """Retain record-local execution order while producing the same independent CSV truth."""
        document = yaml.safe_load((CASE / "spec.yaml").read_text())
        document["rows"] = [
            {
                "id": "records",
                "dataset": "SOURCE",
                "derivations": {
                    column["name"]: column.pop("derivation")
                    for column in document["columns"][1:]
                },
            }
        ]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "spec.yaml"
            path.write_text(yaml.safe_dump(document, sort_keys=False))
            spec = load_specification(path, SCHEMA).specification
        self.assert_csv(self.execute(spec=spec))
        expected = [
            CALLS[offset + row] for row in range(5) for offset in (0, 5, 10, 15)
        ]
        self.assertEqual(self.events, VECTORS + ["source"] + expected)

    def test_unqualified_scopes_are_refused_before_activation(self):
        """Reject grouped, key and secondary function scopes before callback or source effects."""
        for scenario in ("key", "secondary", "grouped"):
            with self.subTest(scenario=scenario):
                spec = self.spec.model_copy(deep=True)
                if scenario == "key":
                    spec = spec.model_copy(update={"keys": ["RATIO"]})
                elif scenario == "secondary":
                    spec.columns[1].derivation.value.root["function"]["args"][
                        "numerator"
                    ] = "OTHER.NUM"
                else:
                    document = yaml.safe_load((CASE / "spec.yaml").read_text())
                    document["rows"] = [
                        {
                            "id": "groups",
                            "dataset": "SOURCE",
                            "group_by": ["SOURCE.ID"],
                            "derivations": {
                                column["name"]: column.pop("derivation")
                                for column in document["columns"][1:]
                            },
                        }
                    ]
                    with tempfile.TemporaryDirectory() as directory:
                        path = Path(directory) / "spec.yaml"
                        path.write_text(yaml.safe_dump(document, sort_keys=False))
                        spec = load_specification(path, SCHEMA).specification
                self.assertEqual(self.execute(spec=spec).result.status, "unsupported")
                self.assertEqual(self.events, [])


class InstalledLockedHostCapabilities(unittest.TestCase):
    def test_actual_yamaa_versions_repeat_without_importing_called_code(self):
        from yamaa import _locked_functions as host

        raw = b'version = 1\n[[package]]\nname = "yamaa"\nversion = "0.2.0"\n'
        with patch.object(
            host.importlib,
            "import_module",
            side_effect=AssertionError("lock verification must not import called code"),
        ):
            for _ in range(2):
                self.assertEqual(
                    host.verify_versions(raw, ["yamaa._native.engine_info"]), ()
                )
            wrong = raw.replace(b'"0.2.0"', b'"9.0"')
            self.assertEqual(
                host.verify_versions(wrong, ["yamaa._native.engine_info"]),
                (host.Finding("yamaa", "version_mismatch", ("9.0",), "0.2.0"),),
            )
        bound = host.resolve_callable("yamaa._native.engine_info", [])
        self.assertIs(bound, yamaa_native.engine_info)
        self.assertEqual(bound()["core_version"], "0.1.0")

    def test_normally_installed_project_wheel_is_checked_before_import_and_binding(
        self,
    ):
        import subprocess
        import sys
        import zipfile

        from yamaa import _locked_functions as host

        module = "yamaa_lock_witness_programs"
        info = "yamaa_lock_witness_programs-1.2.0.dist-info"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "project-imported.txt"
            target = root / "installed"
            wheel = root / "yamaa_lock_witness_programs-1.2.0-py3-none-any.whl"
            source = (
                "from pathlib import Path\n"
                f"Path({str(marker)!r}).write_text('imported', encoding='utf-8')\n"
                "calls = []\n"
                "def calculate(*, x, y=999):\n"
                "    calls.append((x, y))\n"
                "    return x + y\n"
            )
            members = {
                f"{module}/__init__.py": source,
                f"{info}/METADATA": "Metadata-Version: 2.1\nName: yamaa-lock-witness-programs\nVersion: 1.2.0\n",
                f"{info}/WHEEL": "Wheel-Version: 1.0\nGenerator: yamaa-installed-host-witness\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
                f"{info}/top_level.txt": module + "\n",
            }
            record = f"{info}/RECORD"
            members[record] = "".join(name + ",,\n" for name in [*members, record])
            with zipfile.ZipFile(wheel, "w") as archive:
                for name, content in members.items():
                    archive.writestr(name, content)
            installed = subprocess.run(
                [
                    "uv",
                    "pip",
                    "install",
                    "--python",
                    sys.executable,
                    "--target",
                    str(target),
                    "--no-cache",
                    "--offline",
                    "--no-deps",
                    str(wheel),
                ],
                check=False,
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(
                installed.returncode, 0, installed.stdout + installed.stderr
            )
            raw = (
                b'version = 1\n[[package]]\nname = "yamaa"\nversion = "0.2.0"\n'
                b'[[package]]\nname = "yamaa-lock-witness-programs"\nversion = "1.2"\n'
            )
            self.assertNotIn(module, sys.modules)
            with patch.object(sys, "path", [str(target), *sys.path]):
                try:
                    bad = raw.replace(b'"1.2"', b'"9.0"')
                    self.assertEqual(
                        host.verify_versions(bad, [module + ".calculate"]),
                        (
                            host.Finding(
                                "yamaa-lock-witness-programs",
                                "version_mismatch",
                                ("9.0",),
                                "1.2.0",
                            ),
                        ),
                    )
                    self.assertFalse(marker.exists())
                    self.assertNotIn(module, sys.modules)
                    for _ in range(2):
                        self.assertEqual(
                            host.verify_versions(raw, [module + ".calculate"]), ()
                        )
                    self.assertFalse(marker.exists())
                    bound = host.resolve_callable(module + ".calculate", ["x", "y"])
                    self.assertEqual(marker.read_text(encoding="utf-8"), "imported")
                    self.assertEqual(sys.modules[module].calls, [])
                    self.assertEqual(bound(x=1, y=100), 101)
                    self.assertEqual(sys.modules[module].calls, [(1, 100)])
                finally:
                    sys.modules.pop(module, None)


if __name__ == "__main__":
    unittest.main()
