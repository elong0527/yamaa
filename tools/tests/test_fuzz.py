"""The fuzz targets, their seeds, regression inputs and pull request selection."""

import gzip
import io
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path

from tools.ci import fuzz
from tools.paths import ROOT
from tools.tests import support


class FuzzTargets(unittest.TestCase):
    def test_every_target_is_one_cargo_fuzz_binary(self):
        manifest = tomllib.loads((fuzz.FUZZ / "Cargo.toml").read_text())
        self.assertTrue(manifest["package"]["metadata"]["cargo-fuzz"])
        binaries = {
            binary["name"]: binary["path"] for binary in manifest.get("bin", [])
        }
        self.assertEqual(
            binaries, {name: f"fuzz_targets/{name}.rs" for name in fuzz.TARGETS}
        )
        sources = {path.stem for path in (fuzz.FUZZ / "fuzz_targets").glob("*.rs")}
        self.assertEqual(sources, set(fuzz.TARGETS))

    def test_the_fuzz_package_stays_outside_the_shipped_workspace(self):
        # Its own empty [workspace] keeps rust/Cargo.toml from becoming its root
        # and keeps the fuzzing crates out of the canonical lock.
        manifest = tomllib.loads((fuzz.FUZZ / "Cargo.toml").read_text())
        self.assertEqual(manifest["workspace"], {})
        workspace = tomllib.loads((fuzz.WORKSPACE / "Cargo.toml").read_text())
        self.assertNotIn("fuzz", workspace["workspace"]["members"])
        lock = (fuzz.WORKSPACE / "Cargo.lock").read_text()
        self.assertNotIn('name = "libfuzzer-sys"', lock)

    def test_every_target_has_committed_seeds(self):
        for name, target in fuzz.TARGETS.items():
            with self.subTest(target=name):
                self.assertTrue(fuzz.committed(target.seeds))

    def test_every_reader_names_an_existing_file_or_directory(self):
        for name in fuzz.TARGETS:
            for reader in fuzz.readers(name):
                if reader.startswith("rust/fuzz/regressions/"):
                    continue
                with self.subTest(target=name, reader=reader):
                    path = ROOT / reader
                    if reader.endswith("/"):
                        self.assertTrue(path.is_dir())
                    else:
                        self.assertTrue(path.is_file())

    def test_the_workflow_filters_pull_requests_by_every_reader(self):
        self.assertEqual(
            workflow()["on"]["pull_request"]["paths"], fuzz.workflow_paths()
        )

    def test_every_dispatch_choice_fits_the_job_timeout(self):
        # Installing the nightly and cargo-fuzz, building the target and
        # writing its seeds take well under ten minutes on a cold cache;
        # thirty leave room for a slower runner.
        setup_minutes = 30
        definition = workflow()
        seconds = definition["on"]["workflow_dispatch"]["inputs"]["seconds"]
        self.assertEqual(seconds["type"], "choice")
        self.assertIn(seconds["default"], seconds["options"])
        longest = max(int(choice) for choice in seconds["options"])
        timeout = definition["jobs"]["fuzz"]["timeout-minutes"]
        self.assertLessEqual(longest + setup_minutes * 60, timeout * 60)

    def test_a_change_selects_the_targets_that_read_it(self):
        everything = list(fuzz.TARGETS)
        for changed, expected in (
            (
                [
                    "rust/crates/yamaa-adapters/vendor/readstat/sas/readstat_xport_read.c"
                ],
                ["sas7bdat", "xport"],
            ),
            (["rust/crates/yamaa-core/src/typed_csv.rs"], ["csv", "dataset_json"]),
            # Every reader builds its table in the shared column storage.
            (
                ["rust/crates/yamaa-adapters/src/columns.rs"],
                ["csv", "dataset_json", "odm_xml", "parquet", "sas7bdat", "xport"],
            ),
            (["rust/crates/yamaa-adapters/src/parquet_source/reader.rs"], ["parquet"]),
            (["rust/crates/yamaa-adapters/src/odm_xml/archive.rs"], ["odm_xml"]),
            (["rust/crates/yamaa-core/src/odm_items.rs"], ["odm_xml"]),
            (["rust/vendor/saphyr-parser/src/lib.rs"], ["specification"]),
            # The specification target admits the schema files the adapters embed.
            (["rust/crates/yamaa-adapters/schema/schema_spec.yaml"], ["specification"]),
            (["rust/fuzz/fuzz_targets/csv.rs"], ["csv"]),
            (["rust/fuzz/regressions/xport/zero-variables.hex"], ["xport"]),
            (["rust/fuzz/src/lib.rs"], everything),
            (["tools/ci/fuzz.py"], everything),
            (["rust/Cargo.lock"], everything),
            # Every reader enforces the fixed limits and builds the core table
            # and value types, and dataset_json matches them exhaustively.
            (["rust/crates/yamaa-core/src/limits.rs"], everything),
            (["rust/crates/yamaa-core/src/table.rs"], everything),
            (["rust/crates/yamaa-core/src/value.rs"], everything),
            (["rust/crates/yamaa-core/src/specification_run.rs"], everything),
            (["rust/fuzz/README.md", "README.md"], []),
            # A directory reader matches only below it, not a sibling prefix.
            (["rust/crates/yamaa-adapters/src/sas_source_extra.rs"], []),
        ):
            with self.subTest(changed=changed):
                self.assertEqual(fuzz.selected(iter(changed)), expected)


class RegressionInputs(unittest.TestCase):
    def test_hex_text_decodes_without_its_comments(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "case.hex"
            path.write_text("# a comment\n00ff 10\n# another\n41\n", encoding="ascii")
            self.assertEqual(fuzz.regression(path), b"\x00\xff\x10\x41")

    def test_committed_inputs_decode_for_a_known_target(self):
        for path in sorted((fuzz.FUZZ / "regressions").glob("*/*.hex")):
            with self.subTest(path=path.name):
                self.assertIn(path.parent.name, fuzz.TARGETS)
                self.assertTrue(fuzz.regression(path))

    def test_seeds_copy_fixtures_and_regressions_without_extensions(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "seeds"
            count = fuzz.seed("xport", destination)
            fixtures = fuzz.committed(fuzz.TARGETS["xport"].seeds)
            self.assertEqual(count, len(fixtures))
            written = sorted(destination.iterdir())
            self.assertEqual(
                len(written), len(fixtures) + len(fuzz.regressions("xport"))
            )
            self.assertTrue(all(path.suffix == "" for path in written))
            copies = {path.read_bytes() for path in written}
            self.assertTrue(all(path.read_bytes() in copies for path in fixtures))
            with self.assertRaises(FileExistsError):
                fuzz.seed("xport", destination)

    def test_odm_seeds_add_each_fixture_as_the_member_of_tar_archives(self):
        fixtures = fuzz.committed(fuzz.TARGETS["odm_xml"].seeds)
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "seeds"
            self.assertEqual(fuzz.seed("odm_xml", destination), len(fixtures))
            written = sorted(destination.iterdir())
            self.assertEqual(len(written), 5 * len(fixtures))
        for path in fixtures:
            archives = fuzz.archives(path)
            with self.subTest(fixture=path.name):
                # Fixed metadata and timestamps make each archive reproducible.
                self.assertEqual(archives, fuzz.archives(path))
                self.assertEqual(
                    archives["ustar"], gzip.decompress(archives["ustar-gzip"])
                )
                long = fuzz.LONG_MEMBER.format(name=path.name)
                for kind, name, marker in (
                    ("ustar", path.name, None),
                    ("gnu", long, b"././@LongLink"),
                    ("pax", long, b"path="),
                ):
                    with self.subTest(kind=kind, name=name):
                        with tarfile.open(
                            fileobj=io.BytesIO(archives[kind])
                        ) as archive:
                            (member,) = archive.getmembers()
                            self.assertEqual(member.name, name)
                            self.assertEqual(
                                archive.extractfile(member).read(), path.read_bytes()
                            )
                        if marker:
                            # The name exceeds a ustar header's 100 bytes, so a GNU
                            # long-name entry or a PAX path record precedes it.
                            self.assertGreater(len(name.encode()), 100)
                            self.assertIn(marker, archives[kind][:1024])


def workflow():
    return support.workflow("fuzz.yml")


if __name__ == "__main__":
    unittest.main()
