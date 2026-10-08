"""Mutation checks for the dependency guard (including non-runtime edges)."""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "check_dependencies", Path(__file__).parents[1] / "tools/check_dependencies.py"
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class DependencyTests(unittest.TestCase):
    def metadata(self):
        return {
            "packages": [
                {
                    "name": name,
                    "dependencies": [
                        {
                            "name": d,
                            "target": "cfg(unix)",
                            "kind": None,
                            "req": "=1.1.5",
                            "features": ["fs", "process"],
                        }
                        if d == "rustix"
                        else {
                            "name": d,
                            "target": "cfg(windows)",
                            "kind": None,
                            "req": "=0.61.2",
                            "features": guard.WINDOWS_FILE_FEATURES.copy(),
                        }
                        if d == "windows-sys"
                        else {"name": d}
                        for d in deps
                    ],
                }
                for name, deps in guard.ALLOWED.items()
            ]
        }

    def test_allowed_graph(self):
        self.assertEqual(guard.violations(self.metadata()), [])

    def test_rejects_runtime_build_and_dev_edges(self):
        for crate in ("yamaa-core", "yamaa-engine"):
            for kind in (None, "build", "dev"):
                for dependency in (
                    "polars",
                    "arrow",
                    "arrow-array",
                    "arrow-schema",
                    "arrow-buffer",
                    "arrow-ipc",
                    "flatbuffers",
                    "pyo3",
                    "extendr-api",
                    "saphyr-parser",
                    "rustix",
                    "windows-sys",
                ):
                    with self.subTest(crate=crate, kind=kind, dependency=dependency):
                        metadata = self.metadata()
                        package = next(
                            p for p in metadata["packages"] if p["name"] == crate
                        )
                        package["dependencies"].append(
                            {"name": dependency, "kind": kind}
                        )
                        self.assertTrue(guard.violations(metadata))

    def test_rejects_reverse_internal_edge(self):
        metadata = self.metadata()
        metadata["packages"][0]["dependencies"].append({"name": "yamaa-engine"})
        self.assertTrue(guard.violations(metadata))

    def test_filesystem_dependency_cannot_become_portable_build_or_unpinned(self):
        for field, value in [
            ("target", None),
            ("target", "cfg(windows)"),
            ("kind", "build"),
            ("kind", "dev"),
            ("req", "^1"),
            ("features", ["fs"]),
            ("features", ["fs", "process", "net"]),
        ]:
            with self.subTest(field=field, value=value):
                metadata = self.metadata()
                package = next(
                    p for p in metadata["packages"] if p["name"] == "yamaa-adapters"
                )
                dependency = next(
                    d for d in package["dependencies"] if d["name"] == "rustix"
                )
                dependency[field] = value
                self.assertTrue(guard.violations(metadata))

    def test_windows_sdk_cannot_gain_other_targets_features_or_dependency_kinds(self):
        for field, value in [
            ("target", None),
            ("target", "cfg(unix)"),
            ("kind", "build"),
            ("kind", "dev"),
            ("req", "^0.61"),
            ("features", ["Win32_Networking_WinSock"]),
        ]:
            with self.subTest(field=field, value=value):
                metadata = self.metadata()
                package = next(
                    p for p in metadata["packages"] if p["name"] == "yamaa-adapters"
                )
                dependency = next(
                    d for d in package["dependencies"] if d["name"] == "windows-sys"
                )
                dependency[field] = value
                self.assertTrue(guard.violations(metadata))

    def test_rejects_unreviewed_member(self):
        metadata = self.metadata()
        metadata["packages"].append({"name": "extra", "dependencies": []})
        self.assertTrue(guard.violations(metadata))
