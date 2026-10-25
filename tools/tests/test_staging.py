"""Ensure the R archive has real shared sources and cannot overwrite prior work."""

import contextlib
import csv
import io
import json
import os
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import installed
from tools.packaging import check_cargo_lock
from tools.packaging import generate_selftest_cases as selftest_cases
from tools.packaging import generate_shipped_schema as shipped_schema
from tools.staging import engine_bundle, vendor_crates
from tools.staging import stage_grammar_inputs as grammar
from tools.staging import stage_r_package as staging


class StagingTests(unittest.TestCase):
    def test_shared_sources_are_self_contained(self):
        """The staged archive owns sources and fixture bytes without checkout links."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "yamaa"
            with contextlib.redirect_stdout(io.StringIO()):
                staging.stage(destination)
            self.assertTrue((destination / "DESCRIPTION").is_file())
            # configure writes the Makevars from their templates at
            # installation, so the package carries only the templates.
            for name in ("configure", "configure.win", "cleanup", "cleanup.win"):
                self.assertTrue(os.access(destination / name, os.X_OK), name)
            self.assertTrue((destination / "tools/configure.R").is_file())
            for name in ("Makevars", "Makevars.win"):
                self.assertFalse((destination / "src" / name).exists(), name)
                self.assertTrue((destination / "src" / f"{name}.in").is_file(), name)
            # src/rust is the R engine bundle: its sources from the checkout,
            # its manifests without test-only dependencies and a verified
            # trimmed lock, beside the compressed crates and their Cargo
            # configuration.
            embedded = destination / "src/rust"
            expected = {
                relative: engine_bundle.source_bytes(source)
                for relative, source in engine_bundle.sources("r")
            }
            expected["Cargo.toml"] = engine_bundle.workspace_manifest("r").encode()
            self.assertEqual(
                {
                    path.relative_to(embedded).as_posix()
                    for path in embedded.rglob("*")
                    if path.is_file()
                },
                set(expected) | {"Cargo.lock", "vendor.tar.xz", "vendor-config.toml"},
            )
            for relative, payload in expected.items():
                self.assertFalse((embedded / relative).is_symlink())
                self.assertEqual((embedded / relative).read_bytes(), payload)
            # The manifest names no stripping: it leaves out the wheels' dist
            # profile, and the Makevars build with release.
            manifest = (embedded / "Cargo.toml").read_text(encoding="utf-8")
            self.assertNotIn('strip = "symbols"', manifest)
            self.assertIn("release", tomllib.loads(manifest)["profile"])
            self.assertNotIn("dist", tomllib.loads(manifest)["profile"])
            kept = engine_bundle.verify_lock(
                (embedded / "Cargo.lock").read_text(encoding="utf-8"),
                (staging.WORKSPACE / "Cargo.lock").read_text(encoding="utf-8"),
                engine_bundle.members("r"),
            )
            self.assertEqual(
                sorted(path.name for path in (embedded / "crates").iterdir()),
                sorted(engine_bundle.members("r")),
            )
            # No crate tests, nor the benchmark files that they compile in,
            # and no toolchain file anywhere (#3030 D55). The facade carries the
            # selftest cases that it embeds, with the bytes of benchmarks/
            # (#3030 D108).
            self.assertEqual(list(embedded.rglob("tests")), [])
            for relative, source in selftest_cases.case_files():
                self.assertEqual(
                    (embedded / "crates/yamaa/selftest" / relative).read_bytes(),
                    source.read_bytes(),
                )
            # The adapters carry the schema files that they embed, with the
            # bytes of yaml/.
            for relative in shipped_schema.copied():
                self.assertEqual(
                    (embedded / "crates/yamaa-adapters/schema" / relative).read_bytes(),
                    (staging.REPOSITORY / "yaml" / relative).read_bytes(),
                )
            self.assertEqual(
                {path.name for path in (destination / "src").iterdir()},
                {path.name for path in (staging.REPOSITORY / "R/yamaa/src").iterdir()}
                | {"rust"},
            )
            self.assertEqual(
                [
                    path
                    for path in destination.rglob("*")
                    if path.name in vendor_crates.TOOLCHAIN_FILES
                ],
                [],
            )
            # The installed R package carries the Python package notices, with
            # the third-party notices and the component inventory of its own
            # bundle.
            notices = tomllib.loads(
                (staging.REPOSITORY / "python/pyproject.toml").read_text(
                    encoding="utf-8"
                )
            )["project"]["license-files"]
            self.assertIn("LICENSE.txt", notices)
            self.assertIn("LICENSE-THIRD-PARTY.txt", notices)
            self.assertIn("COMPONENTS.tsv", notices)
            for name in notices:
                host = (
                    "R/yamaa/inst" if name in staging.notices.HOST_NOTICES else "python"
                )
                self.assertEqual(
                    (destination / "inst" / name).read_bytes(),
                    (staging.REPOSITORY / host / name).read_bytes(),
                )
            inventory = (destination / "inst/COMPONENTS.tsv").read_text(
                encoding="utf-8"
            )
            self.assertIn("\nyamaa-r\t", inventory)
            self.assertNotIn("\nyamaa-python\t", inventory)
            listed = staging.notices.committed_entries(
                (destination / "inst/LICENSE-THIRD-PARTY.txt").read_text(
                    encoding="utf-8"
                )
            )
            self.assertIn("extendr-api", {name for name, _ in listed})
            self.assertNotIn("pyo3", {name for name, _ in listed})
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
            # R installs inst/, which holds AUTHORS and the notices alone; the
            # inputs that the suites read sit beside them in tests/ (#3105).
            self.assertEqual(
                {path.name for path in (destination / "inst").iterdir()},
                {"AUTHORS", *notices},
            )
            tests = destination / "tests"
            self.assertEqual(
                {path.name for path in tests.iterdir() if path.is_file()},
                {
                    path.name
                    for path in (staging.REPOSITORY / "R/yamaa/tests").glob("*.R")
                },
            )
            self.assertEqual(
                {path.name for path in tests.iterdir() if path.is_dir()},
                {*staging.TEST_INPUTS, "host-fixtures"},
            )
            for source in staging.PROJECTS.rglob("*"):
                if source.is_file():
                    relative = source.relative_to(staging.PROJECTS)
                    self.assertEqual(
                        (tests / "project-base" / relative).read_bytes(),
                        source.read_bytes(),
                    )
            # R CMD check notes a file path that a tar file may not store: one
            # longer than 100 bytes, archive directory included.
            for path in destination.rglob("*"):
                if path.is_file():
                    name = Path("yamaa", path.relative_to(destination)).as_posix()
                    self.assertLessEqual(len(name.encode()), 100, name)
            grammar = tests / "grammar-original/grammar"
            for source in (staging.REPOSITORY / "yaml/grammar").glob("*.yaml"):
                self.assertEqual(
                    (grammar / source.name).read_bytes(), source.read_bytes()
                )
            self.assertTrue((destination / "tests/public_grammar.R").is_file())
            # The package builds offline from the crates.io crates of the
            # trimmed lock, and carries the script that bundled them and
            # their authors' names.
            registry = {(name, version) for name, version, source in kept if source}
            unpacked = Path(directory) / "unpacked"
            with tarfile.open(destination / staging.VENDOR_ARCHIVE, "r:xz") as archive:
                members = archive.getmembers()
                self.assertTrue(all(m.isfile() or m.isdir() for m in members))
                archive.extractall(unpacked, filter="data")
            crates = sorted((unpacked / vendor_crates.DIRECTORY).iterdir())
            self.assertEqual(
                {crate.name for crate in crates},
                {f"{name}-{version}" for name, version in registry},
            )
            stubs = {}
            for crate in crates:
                files = {
                    path.relative_to(crate).as_posix()
                    for path in crate.rglob("*")
                    if path.is_file()
                }
                record = json.loads((crate / ".cargo-checksum.json").read_text())
                # A crate that no supported target compiles keeps its manifest
                # alone, with an empty, unrecorded src/lib.rs where Cargo
                # needs one to read it.
                if set(record["files"]) == {vendor_crates.MANIFEST}:
                    stubs[crate.name] = files - {".cargo-checksum.json"}
                    library = crate / vendor_crates.LIBRARY
                    if library.is_file():
                        self.assertEqual(library.read_bytes(), b"")
                        files.discard(vendor_crates.LIBRARY)
                self.assertEqual(set(record["files"]), files - {".cargo-checksum.json"})
                for path in crate.rglob("*"):
                    if path.is_file():
                        self.assertFalse(vendor_crates.is_binary(path), path)
                        self.assertNotIn(path.name, vendor_crates.TOOLCHAIN_FILES)
            directories = {name: f"{name}-{version}" for name, version in registry}
            self.assertIn(directories["r-efi"], stubs)
            self.assertNotIn(directories["extendr-api"], stubs)
            for remaining in stubs.values():
                self.assertLessEqual(remaining, {"Cargo.toml", vendor_crates.LIBRARY})
            # The packages carry no text of a license that yamaa does not rely on.
            vendored = unpacked / vendor_crates.DIRECTORY
            for name, texts in vendor_crates.ALTERNATIVE_LICENSES.items():
                for text in texts:
                    self.assertFalse((vendored / directories[name] / text).exists())
            self.assertEqual(
                (destination / staging.VENDOR_CONFIG).read_text(encoding="utf-8"),
                engine_bundle.CARGO_CONFIG,
            )
            self.assertEqual(
                (destination / staging.VENDOR_SCRIPT).read_bytes(),
                Path(vendor_crates.__file__).read_bytes(),
            )
            authors = (destination / staging.AUTHORS).read_text(encoding="utf-8")
            self.assertEqual(
                {
                    tuple(line.removeprefix("Crate: ").split(" "))
                    for line in authors.splitlines()
                    if line.startswith("Crate: ")
                },
                registry | {("yamaa-saphyr-parser", "0.2.1")},
            )
            entries = {
                entry.split(" ", 1)[0]: entry for entry in authors.split("\nCrate: ")
            }
            self.assertIn(
                "    Left out: every file but its manifest\n", entries["r-efi"]
            )
            self.assertIn("    Left out: zstd/COPYING\n", entries["zstd-sys"])
            # The notices cover the bundled crates that the R binding links,
            # not the procedural macros or the crates of other platforms, and
            # the inventory names one license each.
            self.assertLess(listed, registry | {("yamaa-saphyr-parser", "0.2.1")})
            names = {name for name, _ in listed}
            self.assertIn("extendr-api", names)
            self.assertNotIn("extendr-macros", names)
            self.assertNotIn("r-efi", names)
            self.assertNotIn(" OR ", authors)
            original = tests / "specification-original"
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
                    / "crates/yamaa-application/tests/fixtures/specs"
                    / (name + ".json")
                )
                self.assertEqual(
                    json.loads((original / "expected" / (name + ".json")).read_text()),
                    json.loads(truth.read_text()),
                )

            # The inputs are what the R suites read, and each third-party
            # fixture keeps its license beside it.
            for path in tests.rglob("*"):
                if path.is_file():
                    relative = path.relative_to(tests).as_posix()
                    left_out = False
                    for pattern in staging.UNREAD_BY_R_SUITES:
                        if staging.matches(relative, pattern.removeprefix("!")):
                            left_out = not pattern.startswith("!")
                    self.assertFalse(left_out, relative)
            self.assertFalse((tests / "host_fixtures.py").exists())
            self.assertFalse((tests / "define-fixtures/schemas").exists())
            self.assertEqual(
                [path.relative_to(tests) for path in tests.rglob("uv.lock")],
                [Path("project-base/negative-function-contract/python/uv.lock")],
            )
            projects = tests / "project-base"
            self.assertFalse((tests / "project-original").exists())
            for path in projects.rglob("*"):
                if path.is_file():
                    self.assertNotIn(b"yamaabenchmarks", path.read_bytes(), str(path))
            for case in ("negative-function-contract", "base-round"):
                lock = json.loads((projects / case / "r/renv.lock").read_text())
                self.assertEqual(set(lock["Packages"]), {"yamaa"})
            # The R callable is named in the R environment file a 0.5 project
            # links, or in the function files a 1.x project lists (D8).
            for case, callable_ in (
                ("negative-function-contract", "base::identity"),
                ("base-round", "base::signif"),
            ):
                callables = staging.r_callables(projects / case)
                self.assertTrue(callables)
                for path in callables:
                    self.assertIn(callable_, path.read_text(), str(path))
            self.assertEqual(list(original.glob("*.json")), [])
            fixtures = staging.WORKSPACE / "crates/yamaa-adapters/tests/fixtures/sas"
            for name in ("LICENSE-haven.txt", "LICENSE-pandas.txt"):
                self.assertEqual(
                    (tests / "host-fixtures/sas" / name).read_bytes(),
                    (fixtures / name).read_bytes(),
                )
            self.assertFalse((tests / "host-fixtures/sas/README.md").exists())
            # Native qualification stages the same inputs beside the installed
            # suites that R CMD check reads from the package's tests directory.
            native = Path(directory) / "native"
            with contextlib.redirect_stdout(io.StringIO()):
                installed.stage("r", native)

            def files(root):
                return {
                    path.relative_to(root).as_posix(): path.read_bytes()
                    for path in root.rglob("*")
                    if path.is_file()
                }

            for name in (*staging.TEST_INPUTS, "host-fixtures"):
                self.assertEqual(files(native / name), files(tests / name), name)
            for suite in (staging.REPOSITORY / "R/yamaa/tests").glob("*.R"):
                self.assertEqual((native / suite.name).read_bytes(), suite.read_bytes())
            # The package passes the check that its built archive gets.
            built = Path(directory) / "yamaa_0.5.0.tar.gz"
            with tarfile.open(built, "w:gz", compresslevel=1) as output:
                output.add(destination, arcname="yamaa")
            self.assertEqual(check_cargo_lock.check_bundle_archive(built), ("r", kept))

    def test_r_callables_are_read_from_the_function_files_a_project_lists(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory)
            (case / "project.yaml").write_text(
                'yamaa: "1.0"\ntype: project\n'
                "functions: [functions/a.yaml, functions/b.yaml]\n"
                "environments: {r: {lock: r/renv.lock}}\n"
            )
            self.assertEqual(
                staging.r_callables(case),
                [case / "functions/a.yaml", case / "functions/b.yaml"],
            )

    def test_shared_grammar_inputs_cover_every_context(self):
        """The hosts replay every case, each in a 1.0 specification, since the
        engine reads only 1.0 specifications and a public field reads each
        context."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "grammar-original"
            grammar.stage(destination)
            with (destination / "cases.tsv").open(
                encoding="utf-8", newline=""
            ) as stream:
                staged = list(csv.DictReader(stream, delimiter="\t"))
            headers = {
                (written["yamaa"], written["type"])
                for row in staged
                for written in [
                    json.loads(
                        (destination / row["specification"]).read_text(encoding="utf-8")
                    )
                ]
            }
        self.assertEqual(headers, {("1.0", "spec")})
        documents = dict(grammar.documents())
        declared = documents["expression"]["contexts"]
        self.assertEqual(list(grammar.CONTEXTS), declared)
        self.assertEqual(
            {row["context"] for row in staged if row["contract"] == "expression"},
            set(declared),
        )
        every = [case for document in documents.values() for case in document["cases"]]
        every += grammar.expansions(documents["expression"])
        self.assertEqual(len(staged), len(every))

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

    def test_existing_test_inputs_are_refused_before_staging(self):
        """Neither the package nor native qualification overwrites a tree."""
        for name in staging.TEST_INPUTS:
            for stage in (
                staging.stage_test_inputs,
                lambda destination: installed.stage("r", destination),
            ):
                with self.subTest(name=name), tempfile.TemporaryDirectory() as root:
                    marker = Path(root) / name / "keep.txt"
                    marker.parent.mkdir()
                    marker.write_text("prior inputs")
                    with self.assertRaises(FileExistsError):
                        stage(Path(root))
                    self.assertEqual(
                        sorted(
                            path.relative_to(root) for path in Path(root).rglob("*")
                        ),
                        [Path(name), Path(name, "keep.txt")],
                    )
