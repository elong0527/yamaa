"""The locked graph builds with the workspace's minimum Rust."""

import tempfile
import unittest
from pathlib import Path

from tools.packaging import check_minimum_rust as check


def package(name, version, edition="2021", rust_version=None):
    return {
        "name": name,
        "version": version,
        "edition": edition,
        "rust_version": rust_version,
    }


class MinimumRustTest(unittest.TestCase):
    def test_a_package_needs_its_rust_version_or_its_edition(self):
        for declared, edition, expected in (
            ("1.70", "2021", "1.70"),
            (None, "2021", "1.56"),
            (None, "2024", "1.85"),
            # An older rust-version than the edition allows reads as the
            # edition's first release.
            ("1.80", "2024", "1.85"),
            ("1.90.1", "2024", "1.90.1"),
            (None, "2015", "1.0"),
        ):
            with self.subTest(declared=declared, edition=edition):
                self.assertEqual(
                    check.needs(package("a", "1.0.0", edition, declared)), expected
                )

    def test_releases_compare_by_number(self):
        self.assertLess(check.release("1.9"), check.release("1.82"))
        self.assertEqual(check.release("1.82"), check.release("1.82.0"))
        self.assertGreater(check.release("1.82.1"), check.release("1.82"))

    def test_only_packages_a_target_compiles_need_the_minimum(self):
        every = {
            "packages": [
                package("compiled", "1.0.0", rust_version="1.82"),
                package("newer", "2.0.0", rust_version="1.85"),
                package("windows", "0.4.0", rust_version="1.86"),
                # A wasm-only crate that no supported target compiles.
                package("wasm", "1.0.4", rust_version="1.87.0"),
                package("unset", "0.1.0", edition="2018"),
            ]
        }
        linux = {("compiled", "1.0.0"), ("newer", "2.0.0"), ("unset", "0.1.0")}
        by_target = {
            "x86_64-unknown-linux-gnu": linux,
            "x86_64-pc-windows-gnu": linux | {("windows", "0.4.0")},
        }
        self.assertEqual(
            check.problems("1.82", every, by_target),
            [
                "These locked packages need a newer Rust than the workspace's "
                "rust-version 1.82: newer 2.0.0 needs 1.85, built for "
                "x86_64-unknown-linux-gnu, x86_64-pc-windows-gnu; windows 0.4.0 "
                "needs 1.86, built for x86_64-pc-windows-gnu"
            ],
        )
        self.assertEqual(check.problems("1.86", every, by_target), [])

    def test_every_locked_package_needs_an_edition_the_minimum_cargo_reads(self):
        # A source package vendors this crate for no supported target, but
        # cargo still reads its manifest.
        every = {"packages": [package("wit-bindgen", "0.57.1", edition="2024")]}
        self.assertEqual(
            check.problems("1.82", every, {"x86_64-unknown-linux-gnu": set()}),
            [
                "Cargo 1.82 cannot read these locked packages, which a source "
                "package vendors: wit-bindgen 0.57.1 (edition 2024)"
            ],
        )
        self.assertEqual(check.problems("1.85", every, {}), [])

    def test_the_minimum_is_the_workspace_rust_version(self):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            (workspace / "Cargo.toml").write_text(
                '[workspace.package]\nrust-version = "1.82"\n', encoding="utf-8"
            )
            self.assertEqual(check.minimum(workspace), "1.82")

    def test_the_targets_are_the_supported_platforms(self):
        # Linux x86_64 and aarch64, macOS arm64, and Windows x64 with MSVC for
        # the wheel and the command line and GNU for the R package.
        self.assertEqual(
            set(check.TARGETS),
            {
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
                "x86_64-pc-windows-gnu",
            },
        )


if __name__ == "__main__":
    unittest.main()
