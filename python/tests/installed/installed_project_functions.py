"""Installed metadata, native project results and original failure retention."""

import json
import tempfile
import unittest
from importlib import metadata
from pathlib import Path
from unittest.mock import patch

import yamaa

ROOT = Path(__file__).parent


class InstalledLockedHostCapabilities(unittest.TestCase):
    def test_actual_yamaa_versions_repeat_without_importing_called_code(self):
        from yamaa import _locked_functions as host

        installed_version = metadata.version("yamaa")
        raw = (
            'version = 1\n[[package]]\nname = "yamaa"\n'
            f'version = "{installed_version}"\n'
        ).encode()
        with patch.object(
            host.importlib,
            "import_module",
            side_effect=AssertionError("lock verification must not import called code"),
        ):
            for _ in range(2):
                self.assertEqual(host.verify_versions(raw, ["yamaa.domain"]), ())
            wrong = raw.replace(
                f'version = "{installed_version}"'.encode(),
                b'version = "999999.0"',
            )
            self.assertEqual(
                host.verify_versions(wrong, ["yamaa.domain"]),
                (
                    host.Finding(
                        "yamaa", "version_mismatch", ("999999.0",), installed_version
                    ),
                ),
            )
        bound = host.resolve_callable("yamaa.domain", ["specification", "environment"])
        self.assertIs(bound, __import__("yamaa").domain)

    def test_normally_installed_project_wheel_is_checked_before_import_and_binding(
        self,
    ):
        import os
        import shutil
        import subprocess
        import sys
        import sysconfig
        import venv

        if os.environ.get("YAMAA_PROJECT_WITNESS_ISOLATED") != "1":
            with tempfile.TemporaryDirectory(
                prefix="yamaa-project-witness-"
            ) as directory:
                venv.EnvBuilder(with_pip=False).create(directory)
                interpreter = Path(directory) / (
                    "Scripts/python.exe" if os.name == "nt" else "bin/python"
                )
                child_packages = subprocess.check_output(
                    [
                        str(interpreter),
                        "-c",
                        "import sysconfig; print(sysconfig.get_paths()['purelib'])",
                    ],
                    text=True,
                ).strip()

                def clone_installed_file(source, destination):
                    try:
                        return os.link(source, destination)
                    except OSError:
                        return shutil.copy2(source, destination)

                shutil.copytree(
                    sysconfig.get_paths()["purelib"],
                    child_packages,
                    copy_function=clone_installed_file,
                    dirs_exist_ok=True,
                )
                subprocess.run(
                    [
                        str(interpreter),
                        __file__,
                        type(self).__name__
                        + ".test_normally_installed_project_wheel_is_checked_before_import_and_binding",
                    ],
                    env=os.environ | {"YAMAA_PROJECT_WITNESS_ISOLATED": "1"},
                    check=True,
                )
            return

        import gc
        import subprocess
        import zipfile

        from yamaa import _locked_functions as host

        module = "yamaa_lock_witness_programs"
        info = "yamaa_lock_witness_programs-1.2.0.dist-info"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "project-imported.txt"
            wheel = root / "yamaa_lock_witness_programs-1.2.0-py3-none-any.whl"
            source = (
                "from pathlib import Path\n"
                f"Path({str(marker)!r}).write_text('imported', encoding='utf-8')\n"
                "calls = []\n"
                "failure = None\n"
                "failure_after = 0\n"
                "def calculate(*, x, y=999):\n"
                "    calls.append((x, y))\n"
                "    if failure is not None and len(calls) > failure_after: raise failure\n"
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
                'version = 1\n[[package]]\nname = "yamaa"\n'
                f'version = "{metadata.version("yamaa")}"\n'
                '[[package]]\nname = "yamaa-lock-witness-programs"\nversion = "1.2"\n'
            ).encode()
            self.assertNotIn(module, sys.modules)
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
                (root / "uv.lock").write_bytes(raw)
                (root / "environment.yaml").write_text(
                    "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions:\n"
                    "  add:\n    function: yamaa_lock_witness_programs.calculate\n"
                    "    description: Add a declared default.\n"
                    "    params: [{name: x, type: int}, {name: y, type: int, required: false, default: 100}]\n"
                    "    returns: int\n    tests:\n"
                    "      - {id: normal, covers: [normal, 'default:y'], args: {x: 1}, result: 101}\n"
                    "      - {id: boundary, covers: [boundary], args: {x: -9223372036854775808}, result: -9223372036854775708}\n"
                    "      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n"
                    "      - {id: missing-y, covers: ['short-circuit-missing:y'], args: {x: 1, y: null}, result: null}\n",
                    encoding="ascii",
                )
                (root / "domain.yaml").write_text(
                    "schema_version: '1.0'\ndomain: TEST\nkeys: [x]\n"
                    "input: {SRC: {path: input.csv, types: {x: int}}}\n"
                    "columns:\n  - {name: x, type: int, derivation: SRC.x}\n"
                    "  - {name: V, type: int, derivation: {function: {name: add, args: {x: SRC.x}}}}\n"
                    "output: {path: result.csv, columns: [x, V]}\n",
                    encoding="ascii",
                )
                (root / "input.csv").write_bytes(b"x\n1\n")
                sys.modules.pop(module)
                marker.unlink()
                specification, environment = (
                    root / "domain.yaml",
                    root / "environment.yaml",
                )
                checked = yamaa.check(specification, environment=environment)
                self.assertTrue(checked.issues.is_empty(), checked.issues)
                self.assertNotIn(module, sys.modules)
                self.assertFalse(marker.exists())
                first = yamaa.domain(specification, environment=environment)
                self.assertTrue(first.issues.is_empty(), first.issues)
                self.assertEqual(
                    first.output.to_dict(as_series=False), {"x": [1], "V": [101]}
                )
                self.assertEqual(marker.read_text(encoding="utf-8"), "imported")
                observed = json.loads(first._native.observations())
                evidence = observed["activation"]
                self.assertEqual(evidence["lock"], "verified")
                self.assertEqual(
                    [test["outcome"] for test in evidence["tests"]], ["passed"] * 4
                )
                self.assertEqual(
                    evidence["tests"][1]["actual"], {"int": "-9223372036854775708"}
                )
                self.assertEqual(evidence["tests"][2]["actual"], {"missing": None})
                self.assertFalse(evidence["tests"][2]["invoked"])
                self.assertEqual(first._native.project_failures(), [])
                self.assertEqual(
                    sys.modules[module].calls,
                    [(1, 100), (-9223372036854775808, 100), (1, 100)],
                )
                self.assertFalse((root / "result.csv").exists())
                for _ in range(2):
                    self.assertTrue(first.save())
                    self.assertEqual(
                        (root / "result.csv").read_bytes(), b"x,V\n1,101\n"
                    )
                self.assertEqual(len(sys.modules[module].calls), 3)
                second = yamaa.domain(specification, environment=environment)
                self.assertTrue(second.issues.is_empty(), second.issues)
                self.assertEqual(
                    second.output.to_dict(as_series=False), {"x": [1], "V": [101]}
                )
                self.assertEqual(len(sys.modules[module].calls), 6)
                self.assertEqual(
                    json.loads(first._native.observations())["activation"], evidence
                )
                (root / "uv.lock").write_bytes(bad)
                rejected = yamaa.domain(specification, environment=environment)
                self.assertIsNone(rejected.output)
                self.assertEqual(
                    rejected.issues["condition"].to_list(),
                    ["runtime_artifact_mismatch"],
                )
                self.assertEqual(
                    json.loads(rejected._native.observations())["source_reads"], []
                )
                self.assertEqual(len(sys.modules[module].calls), 6)
                (root / "uv.lock").write_bytes(raw)
                package = sys.modules[module]
                for original, after, stage in (
                    (ValueError("original conformance failure"), 0, "conformance"),
                    (ValueError("original live failure"), 2, "derivation"),
                ):
                    with self.subTest(stage=stage):
                        package.calls.clear()
                        package.failure, package.failure_after = original, after
                        failed = yamaa.domain(specification, environment=environment)
                        self.assertIsNone(failed.output)
                        failures = failed._native.project_failures()
                        self.assertTrue(failures)
                        self.assertTrue(
                            all(
                                held_stage == stage and held is original
                                for held_stage, held in failures
                            )
                        )
                        report = json.loads(failed._native.observations())
                        issues = failed.issues.to_dicts()
                        self.assertTrue(issues)
                        context = json.loads(issues[0]["context"])
                        if stage == "conformance":
                            self.assertEqual(
                                issues[0]["condition"], "function_conformance_failed"
                            )
                            context = context["failure"]["context"]
                            self.assertEqual(report["source_reads"], [])
                        else:
                            self.assertEqual(
                                issues[0]["condition"], "function_call_failed"
                            )
                            self.assertEqual(
                                [
                                    test["outcome"]
                                    for test in report["activation"]["tests"]
                                ],
                                ["passed"] * 4,
                            )
                            self.assertEqual(len(report["source_reads"]), 1)
                        self.assertEqual(context["host_error"], "ValueError")
                        self.assertEqual(context["host_message"], str(original))
                        gc.collect()
                        self.assertTrue(
                            all(
                                held is original
                                for _, held in failed._native.project_failures()
                            )
                        )
                        with self.assertRaises(yamaa.DomainError):
                            failed.save()
                package.failure = None
                specification.unlink()
                environment.unlink()
                (root / "input.csv").unlink()
                gc.collect()
                for result in (first, second):
                    self.assertEqual(
                        result.output.to_dict(as_series=False), {"x": [1], "V": [101]}
                    )
                    self.assertTrue(result.save())
                    self.assertEqual(
                        (root / "result.csv").read_bytes(), b"x,V\n1,101\n"
                    )
                self.assertEqual(
                    json.loads(first._native.observations())["activation"], evidence
                )
                print(
                    "normally installed project wheel preserves public lock/import/default/i64/missing/callback identity and held saves"
                )
            finally:
                sys.modules.pop(module, None)


if __name__ == "__main__":
    unittest.main()
