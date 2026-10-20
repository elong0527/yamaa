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

from tools.staging import stage_grammar_inputs as grammar
from tools.staging import stage_r_package as staging
from tools.staging import stage_specification_inputs as specification
from tools.staging import vendor_crates


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
            # The package builds offline from the locked crates.io crates, and
            # carries the script that bundled them and their authors' names.
            lock = tomllib.loads((embedded / "Cargo.lock").read_text(encoding="utf-8"))
            registry = {
                (package["name"], package["version"])
                for package in lock["package"]
                if package.get("source", "").startswith("registry+")
            }
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
            for crate in crates:
                self.assertTrue((crate / ".cargo-checksum.json").is_file())
                for path in crate.rglob("*"):
                    if path.is_file():
                        self.assertFalse(vendor_crates.is_binary(path), path)
            self.assertEqual(
                (destination / staging.VENDOR_CONFIG).read_text(encoding="utf-8"),
                vendor_crates.CONFIG,
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

    def test_shared_grammar_inputs_leave_out_only_the_derivation_context(self):
        """The hosts replay every case a public field reads; no field reads
        the derivation context until the engine reads 1.0 files."""
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "grammar-original"
            grammar.stage(destination)
            with (destination / "cases.tsv").open(
                encoding="utf-8", newline=""
            ) as stream:
                staged = list(csv.DictReader(stream, delimiter="\t"))
        documents = dict(grammar.documents())
        declared = documents["expression"]["contexts"]
        self.assertEqual(list(grammar.CONTEXTS), declared)
        self.assertEqual(
            {row["context"] for row in staged if row["contract"] == "expression"},
            set(declared) - {"derivation"},
        )
        every = [case for document in documents.values() for case in document["cases"]]
        every += grammar.expansions(documents["expression"])
        unread = [case for case in every if case.get("context") == "derivation"]
        self.assertTrue(unread)
        self.assertEqual(len(staged), len(every) - len(unread))

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


class NativeFixtureTests(unittest.TestCase):
    """The benchmark files that the R package's crate tests compile in."""

    def compile_fixture(self, root, declared=True):
        workspace = root / "rust"
        name = "benchmarks/original/expected/out.csv"
        original = root / name
        original.parent.mkdir(parents=True)
        original.write_bytes(b"ID\n1\n")
        manifest = workspace / "qualification/native-source-fixtures.json"
        manifest.parent.mkdir(parents=True)
        manifest.write_text(
            json.dumps(
                {
                    "format": "yamaa-native-source-fixtures/1",
                    "files": [name] if declared else [],
                }
            )
        )
        source = workspace / "crates/engine/tests/original.rs"
        source.parent.mkdir(parents=True)
        return workspace, source, original, name

    def test_ordinary_compile_literals_admit_all_macro_delimiters(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, name = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                    with self.subTest(macro=macro, opening=opening):
                        source.write_text(
                            "fn main() { let _ = "
                            + macro
                            + "!"
                            + opening
                            + json.dumps("../../../../" + name)
                            + closing
                            + "; }\n"
                        )
                        selected = staging.native_test_fixtures(workspace)
                        self.assertEqual(selected[1], (name, original))
                        self.assertEqual(selected[1][1].read_bytes(), b"ID\n1\n")

    def test_brace_bracket_compile_literals_refuse_absolute_and_undeclared_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, name = self.compile_fixture(
                Path(directory), declared=False
            )
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("[", "]"), ("{", "}")):
                    for path, reason in (
                        (str(original.resolve()), "unescaped relative paths"),
                        ("../../../../" + name, "not declared"),
                    ):
                        with self.subTest(macro=macro, opening=opening, path=path):
                            source.write_text(
                                "fn main() { let _ = "
                                + macro
                                + "!"
                                + opening
                                + json.dumps(path)
                                + closing
                                + "; }\n"
                            )
                            with self.assertRaisesRegex(ValueError, reason):
                                staging.native_test_fixtures(workspace)

    def test_raw_compile_literals_refuse_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, _ = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                    for hashes in ("", "#", "###"):
                        with self.subTest(macro=macro, opening=opening, hashes=hashes):
                            literal = (
                                "r"
                                + hashes
                                + '"'
                                + str(original.resolve())
                                + '"'
                                + hashes
                            )
                            source.write_text(
                                "fn main() { let _ = "
                                + macro
                                + "!"
                                + opening
                                + literal
                                + closing
                                + "; }\n"
                            )
                            with self.assertRaisesRegex(
                                ValueError, "ordinary string literals"
                            ):
                                staging.native_test_fixtures(workspace)

    def test_dynamic_and_nonliteral_compile_arguments_refuse_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, _, _ = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                    for value in (
                        'concat!("../../../../", "benchmarks/original/expected/out.csv")',
                        'env!("YAMAA_GUARD_CONTROL_PATH")',
                        "SOURCE_PATH",
                    ):
                        with self.subTest(macro=macro, opening=opening, value=value):
                            source.write_text(
                                "fn main() { let _ = "
                                + macro
                                + "!"
                                + opening
                                + value
                                + closing
                                + "; }\n"
                            )
                            with self.assertRaisesRegex(
                                ValueError, "ordinary string literals"
                            ):
                                staging.native_test_fixtures(workspace)

    def test_comment_separated_compile_tokens_refuse_before_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, _ = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                for gap in ("/* gap */", "/* outer /* inner */ gap */", "// gap\n"):
                    for opening, closing in (("(", ")"), ("[", "]"), ("{", "}")):
                        for before_bang in (True, False):
                            with self.subTest(
                                macro=macro,
                                gap=gap,
                                opening=opening,
                                before_bang=before_bang,
                            ):
                                tokens = (
                                    macro + gap + "!" + opening
                                    if before_bang
                                    else macro + "!" + gap + opening
                                )
                                source.write_text(
                                    "fn main() { let _ = "
                                    + tokens
                                    + json.dumps(str(original.resolve()))
                                    + closing
                                    + "; }\n"
                                )
                                with self.assertRaisesRegex(
                                    ValueError, "ordinary string literals"
                                ):
                                    staging.native_test_fixtures(workspace)

    def test_spaced_bang_arguments_follow_literal_admission(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace, source, original, name = self.compile_fixture(Path(directory))
            for macro in ("include_str", "include_bytes"):
                with self.subTest(macro=macro, kind="ordinary"):
                    source.write_text(
                        "fn main() { let _ = "
                        + macro
                        + " ! ("
                        + json.dumps("../../../../" + name)
                        + "); }\n"
                    )
                    selected = staging.native_test_fixtures(workspace)
                    self.assertEqual(selected[1], (name, original))
                for value in (
                    'r"' + str(original.resolve()) + '"',
                    'concat!("../../../../", "benchmarks/original/expected/out.csv")',
                ):
                    with self.subTest(macro=macro, value=value):
                        source.write_text(
                            "fn main() { let _ = " + macro + " ! (" + value + "); }\n"
                        )
                        with self.assertRaisesRegex(
                            ValueError, "ordinary string literals"
                        ):
                            staging.native_test_fixtures(workspace)

    def test_absolute_and_drive_compile_paths_cannot_read_the_original_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            for path in (
                str(original.resolve()),
                "C:/original/out.csv",
                r"C:\original\out.csv",
            ):
                with self.subTest(path=path):
                    source.write_text(
                        f"const INPUT: &str = include_str!({json.dumps(path)});\n"
                    )
                    with self.assertRaisesRegex(ValueError, "unescaped relative paths"):
                        staging.native_test_fixtures(workspace)

    def test_compile_symlink_alias_is_not_replaced_by_its_declared_regular_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            alias = root / "benchmarks/alias"
            try:
                alias.symlink_to(original.parent, target_is_directory=True)
            except OSError as error:
                self.skipTest(f"compile-input symlink witness is unavailable: {error}")
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                'const INPUT: &str = include_str!("../../../../benchmarks/alias/out.csv");\n'
            )
            with self.assertRaisesRegex(
                ValueError, "compile fixtures must not be symlinks"
            ):
                staging.native_test_fixtures(workspace)

    def test_external_compile_fixture_must_be_declared_before_building_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            source = workspace / "crates/engine/tests/original.rs"
            source.parent.mkdir(parents=True)
            name = "benchmarks/original/expected/out.csv"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"ID\n1\n")
            source.write_text(
                "const ORIGINAL: &str = include_str!(\n"
                '    "../../../../benchmarks/original/expected/out.csv"\n);\n'
            )
            manifest.write_text(
                json.dumps({"format": "yamaa-native-source-fixtures/1", "files": []})
            )
            with self.assertRaisesRegex(ValueError, "compile fixture is not declared"):
                staging.native_test_fixtures(workspace)
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            selected = staging.native_test_fixtures(workspace)
            self.assertEqual(selected[1][0], name)
            self.assertEqual(selected[1][1].read_bytes(), b"ID\n1\n")

    def test_native_fixture_manifest_selects_exact_regular_original_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workspace = root / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            name = "benchmarks/independent/expected/document.xml"
            original = root / name
            original.parent.mkdir(parents=True)
            original.write_bytes(b"independent fixed-time document\n")
            (original.parent / "unselected.csv").write_bytes(b"not selected\n")
            manifest.write_text(
                json.dumps(
                    {"format": "yamaa-native-source-fixtures/1", "files": [name]}
                )
            )
            selected = staging.native_test_fixtures(workspace)
            self.assertEqual(
                [name for name, _ in selected],
                ["rust/qualification/native-source-fixtures.json", name],
            )
            self.assertEqual(
                selected[1][1].read_bytes(), b"independent fixed-time document\n"
            )
            original.unlink()
            with self.assertRaisesRegex(ValueError, "missing"):
                staging.native_test_fixtures(workspace)

    def test_native_fixture_manifest_refuses_ambiguous_or_unselected_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory) / "rust"
            manifest = workspace / "qualification/native-source-fixtures.json"
            manifest.parent.mkdir(parents=True)
            for files in (
                ["../private"],
                ["/benchmarks/private"],
                ["benchmarks/../private"],
                ["benchmarks/a\\b"],
                ["benchmarks/a:stream"],
                ["benchmarks/a", "benchmarks/a"],
            ):
                with self.subTest(files=files):
                    manifest.write_text(
                        json.dumps(
                            {"format": "yamaa-native-source-fixtures/1", "files": files}
                        )
                    )
                    with self.assertRaisesRegex(ValueError, "fixture path"):
                        staging.native_test_fixtures(workspace)

    def test_native_fixture_scan_ignores_ancestor_build_or_cache_names(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "target" / "checkout"
            root.mkdir(parents=True)
            workspace, source, original, name = self.compile_fixture(root)
            source.write_text(
                "fn main() { let _ = include_str!("
                + json.dumps(str(original.resolve()))
                + "); }\n"
            )
            with self.assertRaisesRegex(ValueError, "unescaped relative paths"):
                staging.native_test_fixtures(workspace)
            source.write_text(
                "fn main() { let _ = include_str!("
                + json.dumps("../../../../" + name)
                + "); }\n"
            )
            selected = staging.native_test_fixtures(workspace)
            self.assertEqual(selected[1], (name, original))
