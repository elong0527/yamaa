"""Ensure the R archive has real shared sources and cannot overwrite prior work."""

import contextlib
import csv
import io
import json
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from tools.staging import stage_r_package as staging
from tools.staging import stage_specification_inputs as specification


class StagingTests(unittest.TestCase):
    def test_shared_sources_are_self_contained(self):
        """The staged archive owns sources and fixture bytes without checkout links."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "yamaa"
            with contextlib.redirect_stdout(io.StringIO()):
                staging.stage(destination)
            self.assertTrue((destination / "DESCRIPTION").is_file())
            embedded = destination / "src/rust"
            for source in (staging.WORKSPACE / "crates").rglob("*"):
                if (
                    source.is_file()
                    and source.name != "Cargo.lock"
                    and not any(
                        part in {"target", "__pycache__"} for part in source.parts
                    )
                    and source.suffix != ".pyc"
                ):
                    copied = embedded / source.relative_to(staging.WORKSPACE)
                    self.assertFalse(copied.is_symlink())
                    self.assertEqual(source.read_bytes(), copied.read_bytes())
            # The unchanged declaration names only what a build needs.
            self.assertEqual(
                (embedded / "rust-toolchain.toml").read_bytes(),
                (staging.WORKSPACE / "rust-toolchain.toml").read_bytes(),
            )
            for source in (staging.WORKSPACE / "vendor").rglob("*"):
                if source.is_file():
                    copied = embedded / source.relative_to(staging.WORKSPACE)
                    self.assertFalse(copied.is_symlink())
                    self.assertEqual(source.read_bytes(), copied.read_bytes())
            # The installed R package carries the Python package notices.
            notices = tomllib.loads(
                (staging.REPOSITORY / "python/pyproject.toml").read_text(
                    encoding="utf-8"
                )
            )["project"]["license-files"]
            self.assertIn("LICENSE.txt", notices)
            self.assertIn("LICENSE-THIRD-PARTY.txt", notices)
            for name in notices:
                self.assertEqual(
                    (destination / "inst" / name).read_bytes(),
                    (staging.REPOSITORY / "python" / name).read_bytes(),
                )
            self.assertEqual(
                (destination / "inst/LICENSE.txt").read_bytes(),
                (staging.REPOSITORY / "LICENSE").read_bytes(),
            )
            for author in ("ChenYuheng", "Ethiraric"):
                self.assertIn(
                    (
                        staging.WORKSPACE
                        / f"vendor/saphyr-parser/LICENSES/{author}-MIT"
                    ).read_bytes(),
                    (destination / "inst/LICENSE-YAML.txt").read_bytes(),
                )
            self.assertFalse(
                any(
                    path.name.startswith(".")
                    for path in (embedded / "vendor/saphyr-parser").rglob("*")
                )
            )
            self.assertFalse((embedded / "target").exists())
            grammar = destination / "inst/grammar-original/grammar"
            for source in (staging.REPOSITORY / "yaml/grammar").glob("*.yaml"):
                self.assertEqual(
                    (grammar / source.name).read_bytes(), source.read_bytes()
                )
            self.assertTrue((destination / "tests/public_grammar.R").is_file())
            self.assertEqual(
                (embedded / "Cargo.lock").read_bytes(),
                (staging.WORKSPACE / "Cargo.lock").read_bytes(),
            )
            original = destination / "inst/specification-original"
            for module in (original / "schema").glob("*.yaml"):
                self.assertEqual(
                    module.read_bytes(),
                    (staging.REPOSITORY / "yaml" / module.name).read_bytes(),
                )
            for name in (
                "negative-zero-division",
                "negative-integer-overflow",
                "adam-adlb-ordered-sum",
                "schema-lookup",
                "schema-window-functions",
                "schema-inheritance",
            ):
                files = (
                    (
                        "spec_study.yaml",
                        "spec_organization.yaml",
                        "spec_compound.yaml",
                        "input/lb.csv",
                        "expected/adlb.csv",
                    )
                    if name == "schema-inheritance"
                    else (
                        "spec.yaml",
                        "input/dm.csv",
                        "input/ae.csv",
                        "input/meddict.csv",
                        "expected/adsl.csv",
                    )
                    if name == "schema-lookup"
                    else ("spec.yaml", "input/vs.csv", "expected/advs.csv")
                    if name == "schema-window-functions"
                    else (
                        "spec.yaml",
                        "input/lb.csv",
                        "expected/adlb.csv"
                        if name == "adam-adlb-ordered-sum"
                        else "expected/error.yaml",
                    )
                )
                for relative in files:
                    self.assertEqual(
                        (original / "cases" / name / relative).read_bytes(),
                        (
                            staging.REPOSITORY / "benchmarks" / name / relative
                        ).read_bytes(),
                    )
                truth = (
                    staging.WORKSPACE
                    / "crates/yamaa-adapters/tests/fixtures/specs"
                    / (name + ".json")
                )
                self.assertEqual(
                    json.loads((original / "expected" / (name + ".json")).read_text()),
                    json.loads(truth.read_text()),
                )

            # Installed inputs are what the installed R suites read, and each
            # third-party fixture keeps its license beside it.
            resources = destination / "inst"
            for path in resources.rglob("*"):
                if path.is_file():
                    relative = path.relative_to(resources).as_posix()
                    left_out = False
                    for pattern in staging.UNREAD_BY_R_SUITES:
                        if staging.matches(relative, pattern.removeprefix("!")):
                            left_out = not pattern.startswith("!")
                    self.assertFalse(left_out, relative)
            self.assertFalse((resources / "host_fixtures.py").exists())
            self.assertFalse((resources / "define-fixtures/schemas").exists())
            self.assertEqual(
                [path.relative_to(resources) for path in resources.rglob("uv.lock")],
                [Path("project-base/negative-function-contract/python/uv.lock")],
            )
            projects = resources / "project-base"
            self.assertFalse((resources / "project-original").exists())
            for path in projects.rglob("*"):
                if path.is_file():
                    self.assertNotIn(b"yamaabenchmarks", path.read_bytes(), str(path))
            for case in ("negative-function-contract", "base-round"):
                lock = json.loads((projects / case / "r/renv.lock").read_text())
                self.assertEqual(set(lock["Packages"]), {"yamaa"})
            self.assertIn(
                "base::identity",
                (
                    projects / "negative-function-contract/r/environment.yaml"
                ).read_text(),
            )
            self.assertIn(
                "base::signif", (projects / "base-round/r/environment.yaml").read_text()
            )
            self.assertEqual(list(original.glob("*.json")), [])
            fixtures = staging.WORKSPACE / "crates/yamaa-adapters/tests/fixtures/sas"
            for name in ("LICENSE-haven.txt", "LICENSE-pandas.txt"):
                self.assertEqual(
                    (resources / "host-fixtures/sas" / name).read_bytes(),
                    (fixtures / name).read_bytes(),
                )
            self.assertFalse((resources / "host-fixtures/sas/README.md").exists())

    def test_shared_specification_inputs_keep_both_preparation_copies(self):
        """Both copies retain the fixture-owned facts and requirement citations."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "specification-original"
            specification.stage(destination)
            source_preparation = (
                staging.WORKSPACE
                / "crates/yamaa-adapters/tests/fixtures/inheritance_preparation.tsv"
            )
            retained_preparation = destination / "inheritance-preparation.tsv"
            self.assertEqual(
                retained_preparation.read_bytes(), source_preparation.read_bytes()
            )

            def preparation_rows(path):
                with path.open(encoding="ascii", newline="") as stream:
                    return list(csv.DictReader(stream, delimiter="\t"))

            historical = preparation_rows(retained_preparation)
            normative = preparation_rows(
                retained_preparation.with_name("normative-inheritance-preparation.tsv")
            )
            for rows in (historical, normative):
                for row in rows:
                    row["expected"] = json.loads(row["expected"])
            self.assertEqual(historical, normative)

    def test_unread_inputs_follow_gitignore_order_and_refuse_stale_patterns(self):
        with tempfile.TemporaryDirectory() as directory:
            resources = Path(directory)
            for name in (
                "cases/kept/spec.yaml",
                "cases/held/spec.yaml",
                "cases/held/input/data.csv",
                "pq/read.parquet",
                "pq/unread.parquet",
                "truth.json",
                "deep/truth.json",
            ):
                (resources / name).parent.mkdir(parents=True, exist_ok=True)
                (resources / name).write_text(name)
            staging.leave_out_unread_inputs(
                resources,
                ("cases/held/**", "pq/*", "!pq/read.*", "*.json"),
            )
            self.assertEqual(
                sorted(
                    path.relative_to(resources).as_posix()
                    for path in resources.rglob("*")
                ),
                [
                    "cases",
                    "cases/kept",
                    "cases/kept/spec.yaml",
                    "deep",
                    "deep/truth.json",
                    "pq",
                    "pq/read.parquet",
                ],
            )
            # A stale pattern is refused before any staged file is removed.
            for stale in ("cases/held/**", "!pq/unread.parquet"):
                with (
                    self.subTest(stale=stale),
                    self.assertRaisesRegex(ValueError, "matches no staged file"),
                ):
                    staging.leave_out_unread_inputs(resources, ("pq/*", stale))
                self.assertTrue((resources / "pq/read.parquet").is_file())

    def test_missing_lock_refuses_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                mock.patch.object(staging, "WORKSPACE", root),
                self.assertRaisesRegex(FileNotFoundError, "canonical Cargo.lock"),
            ):
                staging.stage(root / "destination")
            self.assertFalse((root / "destination").exists())

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            marker = destination / "keep.txt"
            marker.write_text("prior build")
            with self.assertRaises(FileExistsError):
                staging.stage(destination)
            self.assertEqual(marker.read_text(), "prior build")
