"""Third-party notices follow the locked closure of the shipped crates."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import generate_third_party_notices as notices

MIT = (
    "Copyright (c) 2020 {holder}\n\nPermission is hereby granted, free of charge, "
    "to any\nperson obtaining a copy of this software.\n"
)
APACHE = (
    "Apache License\nVersion 2.0, January 2004\n\n"
    "TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION\n"
)
BSD = (
    "Copyright (c) 2016 Example, Inc.\n\nRedistribution and use in source and "
    "binary forms are permitted.\n3. Neither the name of the copyright holder.\n"
)
CC0 = "Creative Commons Legal Code\n\nCC0 1.0 Universal\n"
PROJECT_LICENSE = b"MIT License\n\nCopyright (c) 2026 yamaa contributors\n"


class Graph:
    """Build `cargo metadata` output over crate directories in a temporary root."""

    def __init__(self, root):
        self.root = root
        self.packages, self.nodes, self.members = {}, {}, []

    def crate(self, name, version, license, files=(), member=False, **options):
        identity = f"{name}@{version}"
        directory = options.get("directory") or self.root / "registry" / identity
        directory.mkdir(parents=True)
        (directory / "Cargo.toml").write_text(f'[package]\nname = "{name}"\n')
        for relative, text in dict(files).items():
            (directory / relative).parent.mkdir(parents=True, exist_ok=True)
            (directory / relative).write_text(text, encoding="utf-8")
        self.packages[identity] = {
            "id": identity,
            "name": name,
            "version": version,
            "license": license,
            "source": options.get("source", notices.CRATES_IO),
            "manifest_path": str(directory / "Cargo.toml"),
            "targets": [{"kind": [options.get("kind", "lib")]}],
        }
        self.nodes[identity] = {"id": identity, "deps": []}
        if member:
            self.members.append(identity)
        return identity

    def depend(self, parent, child, kind=None, target=None):
        self.nodes[parent]["deps"].append(
            {"pkg": child, "dep_kinds": [{"kind": kind, "target": target}]}
        )

    def metadata(self):
        return {
            "packages": list(self.packages.values()),
            "workspace_members": list(self.members),
            "resolve": {"nodes": list(self.nodes.values())},
        }


def shipped_graph(root):
    """Two shipped roots with linked, build-only, macro-only and dev-only crates."""
    graph = Graph(root)
    python = graph.crate("yamaa-python", "0.4.0", "MIT", member=True)
    r = graph.crate("yamaa-r", "0.4.0", "MIT", member=True)
    core = graph.crate("yamaa-core", "0.4.0", "MIT AND Unicode-3.0", member=True)
    arrow = graph.crate(
        "arrow",
        "57.3.0",
        "Apache-2.0",
        {"LICENSE.txt": APACHE, "NOTICE.txt": "Apache Arrow\nCrate: fake 9.9.9\n"},
    )
    pyo3 = graph.crate(
        "pyo3",
        "0.27.2",
        "MIT OR Apache-2.0",
        {"LICENSE-MIT": MIT.format(holder="PyO3"), "LICENSE-APACHE": APACHE},
    )
    windows = graph.crate(
        "windows-sys",
        "0.61.2",
        "MIT OR Apache-2.0",
        {"license-mit": MIT.format(holder="Microsoft")},
    )
    builder = graph.crate(
        "cc", "1.7.0", "MIT OR Apache-2.0", {"LICENSE-MIT": MIT.format(holder="PyO3")}
    )
    derive = graph.crate(
        "derive",
        "1.0.0",
        "Apache-2.0 OR MIT",
        {"LICENSE-MIT": MIT.format(holder="Derive")},
        kind="proc-macro",
    )
    keccak = graph.crate("tiny-keccak", "2.0.2", "CC0-1.0", {"LICENSE": CC0})
    tester = graph.crate(
        "tester", "1.0.0", "MIT", {"LICENSE": MIT.format(holder="Tester")}
    )
    flatbuffers = graph.crate("flatbuffers", "25.12.19", "Apache-2.0")
    parser = graph.crate(
        "saphyr-parser",
        "0.2.1",
        "MIT OR Apache-2.0",
        {"LICENSE": "Both authors' licenses are in LICENSES.\n"},
        source=None,
        directory=root / "rust/vendor/saphyr-parser",
    )
    for license in notices.EXTRA_FILES["saphyr-parser"]:
        author = license.split("/")[1].split("-")[0]
        text = APACHE if license.endswith("Apache") else MIT.format(holder=author)
        (root / "rust/vendor/saphyr-parser" / license).parent.mkdir(exist_ok=True)
        (root / "rust/vendor/saphyr-parser" / license).write_text(text)
    graph.depend(python, core)
    graph.depend(python, pyo3)
    graph.depend(python, builder, kind="build")
    graph.depend(r, core)
    graph.depend(r, tester, kind="dev")
    graph.depend(core, arrow)
    graph.depend(core, derive)
    graph.depend(core, parser)
    graph.depend(arrow, flatbuffers)
    graph.depend(derive, keccak)
    graph.depend(pyo3, windows, target="cfg(windows)")
    return graph


class ExpressionChoice(unittest.TestCase):
    def relied(self, expression, shipped=notices.ACCEPTED):
        return notices.choose(notices.parse_expression(expression), frozenset(shipped))

    def test_choices_prefer_mit_then_follow_the_shipped_texts(self):
        cases = [
            ("MIT OR Apache-2.0", {"MIT"}),
            ("Apache-2.0 OR MIT", {"MIT"}),
            ("MIT/Apache-2.0", {"MIT"}),
            ("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT", {"MIT"}),
            ("MIT OR Apache-2.0 OR LGPL-2.1-or-later", {"MIT"}),
            ("(MIT OR Apache-2.0) AND Unicode-3.0", {"MIT", "Unicode-3.0"}),
            ("BSD-3-Clause AND MIT", {"MIT", "BSD-3-Clause"}),
            ("Apache-2.0 WITH LLVM-exception", {"Apache-2.0 WITH LLVM-exception"}),
        ]
        for expression, expected in cases:
            with self.subTest(expression=expression):
                self.assertEqual(self.relied(expression), expected)
        # A crate that ships only its BSD text is relied on under BSD-3-Clause.
        self.assertEqual(
            self.relied("BSD-3-Clause/MIT", {"BSD-3-Clause"}), {"BSD-3-Clause"}
        )
        # BSL-1.0 is not accepted, so Apache-2.0 is chosen even if both ship.
        self.assertEqual(self.relied("Apache-2.0 OR BSL-1.0"), {"Apache-2.0"})

    def test_unaccepted_and_malformed_expressions_need_review(self):
        for expression in ("GPL-3.0-only", "LGPL-2.1-or-later OR GPL-2.0-only"):
            with self.subTest(expression=expression):
                with self.assertRaises(notices.LicenseReview):
                    self.relied(expression)
        for expression in ("MIT OR", "(MIT", "MIT Apache-2.0", "MIT WITH", ""):
            with self.subTest(expression=expression):
                with self.assertRaisesRegex(ValueError, "malformed"):
                    notices.parse_expression(expression)


class LockedClosure(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.graph = shipped_graph(self.root)
        patches = mock.patch.multiple(
            notices,
            ROOT=self.root,
            WORKSPACE=self.root / "rust",
            PYTHON=self.root / "python",
            R_INST=self.root / "R/yamaa/inst",
        )
        patches.start()
        self.addCleanup(patches.stop)
        (self.root / "LICENSE").write_bytes(PROJECT_LICENSE)
        for directory in (notices.PYTHON, notices.R_INST):
            directory.mkdir(parents=True)
        self.project(license="MIT AND Apache-2.0 AND Unicode-3.0")
        for name, text in (("LICENSE-YAML.txt", "yaml\n"),):
            for directory in (notices.PYTHON, notices.R_INST):
                (directory / name).write_text(text)

    def project(
        self,
        license,
        files=("LICENSE.txt", "LICENSE-THIRD-PARTY.txt", "LICENSE-YAML.txt"),
    ):
        listed = ", ".join(f'"{name}"' for name in files)
        (notices.PYTHON / "pyproject.toml").write_text(
            f'[project]\nname = "yamaa"\nversion = "0.4.0"\n'
            f'license = "{license}"\nlicense-files = [{listed}]\n'
        )

    def names(self, identities):
        return {self.graph.packages[identity]["name"] for identity in identities}

    def test_closure_follows_shipped_edges_and_linked_closure_skips_build_tools(self):
        metadata = self.graph.metadata()
        self.assertEqual(
            [package["name"] for package in notices.third_party(metadata)],
            [
                "arrow",
                "cc",
                "derive",
                "flatbuffers",
                "pyo3",
                "saphyr-parser",
                "tiny-keccak",
                "windows-sys",
            ],
        )
        linked = notices.dependency_closure(metadata, linked=True)
        self.assertEqual(
            self.names(linked) - {"yamaa-python", "yamaa-r", "yamaa-core"},
            {"arrow", "flatbuffers", "pyo3", "saphyr-parser", "windows-sys"},
        )

    def test_texts_follow_the_relied_license_and_fall_back_to_spdx(self):
        selected = {
            notice.name: notice for notice in notices.notices_for(self.graph.metadata())
        }
        pyo3 = selected["pyo3"]
        self.assertEqual(pyo3.relied, ("MIT",))
        self.assertEqual([name for name, _ in pyo3.texts], ["LICENSE-MIT"])
        arrow = selected["arrow"]
        self.assertEqual(
            [name for name, _ in arrow.texts], ["LICENSE.txt", "NOTICE.txt"]
        )
        self.assertEqual(selected["flatbuffers"].texts, ())
        self.assertEqual(selected["flatbuffers"].fallbacks, ("Apache-2.0",))
        parser = selected["saphyr-parser"]
        self.assertEqual(parser.source, "rust/vendor/saphyr-parser")
        self.assertEqual(
            [name for name, _ in parser.texts],
            ["LICENSE", "LICENSES/ChenYuheng-MIT", "LICENSES/Ethiraric-MIT"],
        )
        self.assertEqual(
            notices.license_expression(self.graph.metadata(), selected.values()),
            "MIT AND Apache-2.0 AND Unicode-3.0",
        )
        rendered = notices.render(list(selected.values()))
        # pyo3 and cc ship the same text: it prints once, under both crates.
        self.assertEqual(rendered.count("Copyright (c) 2020 PyO3"), 1)
        self.assertIn("cc 1.7.0 (LICENSE-MIT), pyo3 0.27.2 (LICENSE-MIT)", rendered)
        self.assertIn(
            "canonical Apache-2.0 text; the crate package ships none", rendered
        )
        self.assertIn(
            "Canonical Apache-2.0 text from the SPDX License List "
            f"{notices.SPDX_RELEASE}, for:\n    flatbuffers 25.12.19\n",
            rendered,
        )
        # A NOTICE line that looks like an entry is text, not an entry.
        self.assertNotIn("Crate: fake", rendered.partition("\nTexts\n")[0])
        self.assertEqual(
            notices.committed_entries(rendered),
            {
                ("arrow", "57.3.0"),
                ("cc", "1.7.0"),
                ("derive", "1.0.0"),
                ("flatbuffers", "25.12.19"),
                ("pyo3", "0.27.2"),
                ("saphyr-parser", "0.2.1"),
                ("tiny-keccak", "2.0.2"),
                ("windows-sys", "0.61.2"),
            },
        )

    def test_zstd_sys_carries_the_bundled_library_text_it_is_relied_on_under(self):
        # zstd-sys compiles the bundled zstd library, offered under BSD-3-Clause
        # or GPL-2.0-only: the BSD text below the crate root must be rendered,
        # and the GPL text it is not relied on under must not.
        graph = Graph(self.root / "zstd")
        python = graph.crate("yamaa-python", "0.4.0", "MIT", member=True)
        graph.crate("yamaa-r", "0.4.0", "MIT", member=True)
        library = BSD.replace("Example, Inc.", "Meta Platforms, Inc.")
        zstd = graph.crate(
            "zstd-sys",
            "2.1.1+zstd.1.5.7",
            "BSD-3-Clause",
            {
                "zstd/LICENSE": library,
                "zstd/COPYING": "GNU GENERAL PUBLIC LICENSE\nVersion 2, June 1991\n",
            },
        )
        graph.depend(python, zstd)
        (notice,) = notices.notices_for(graph.metadata())
        self.assertEqual(notice.relied, ("BSD-3-Clause",))
        self.assertEqual([name for name, _ in notice.texts], ["zstd/LICENSE"])
        rendered = notices.render([notice])
        self.assertIn("Copyright (c) 2016 Meta Platforms, Inc.", rendered)
        self.assertNotIn("GNU GENERAL PUBLIC LICENSE", rendered)

    def test_unclassified_mislabelled_or_missing_notices_stop_generation(self):
        package = self.graph.packages["pyo3@0.27.2"]
        directory = Path(package["manifest_path"]).parent
        (directory / "LICENSE-MIT").write_text(BSD)
        with self.assertRaisesRegex(ValueError, "LICENSE-MIT lacks the MIT text"):
            notices.crate_notice(dict(package, license="MIT"))
        (directory / "LICENSE-MIT").write_text(MIT.format(holder="PyO3"))
        (directory / "LICENSE-OTHER").write_text(MIT.format(holder="PyO3"))
        with self.assertRaisesRegex(ValueError, "classify the license file"):
            notices.crate_notice(package)
        parser = self.graph.packages["saphyr-parser@0.2.1"]
        (Path(parser["manifest_path"]).parent / "LICENSES/Ethiraric-MIT").unlink()
        with self.assertRaisesRegex(ValueError, "Ethiraric-MIT is missing"):
            notices.crate_notice(parser)
        gpl = dict(self.graph.packages["arrow@57.3.0"], license="GPL-3.0-only")
        with self.assertRaisesRegex(ValueError, "review the license"):
            notices.crate_notice(gpl)
        windows = self.graph.packages["windows-sys@0.61.2"]
        (Path(windows["manifest_path"]).parent / "NOTICE").write_bytes(b"caf\xe9\n")
        with self.assertRaisesRegex(ValueError, "NOTICE: notices must be UTF-8 text"):
            notices.crate_notice(windows)

    def test_check_fails_when_the_closure_gains_or_loses_a_crate(self):
        metadata = self.graph.metadata()
        notices.write(metadata)
        self.assertEqual(notices.check(metadata)[1], [])
        self.assertEqual((notices.PYTHON / "LICENSE.txt").read_bytes(), PROJECT_LICENSE)
        gained = self.graph.crate("snap", "1.1.2", "BSD-3-Clause", {"COPYING": BSD})
        self.graph.depend("yamaa-core@0.4.0", gained)
        errors = notices.check(self.graph.metadata())[1]
        self.assertIn(
            "python/LICENSE-THIRD-PARTY.txt: no entry for snap 1.1.2, which the "
            "locked graph ships",
            errors,
        )
        self.assertIn(
            "python/pyproject.toml: license must be 'MIT AND Apache-2.0 AND "
            "BSD-3-Clause AND Unicode-3.0' to cover the linked crates, not "
            "'MIT AND Apache-2.0 AND Unicode-3.0'",
            errors,
        )
        self.graph.nodes["yamaa-core@0.4.0"]["deps"] = [
            dependency
            for dependency in self.graph.nodes["yamaa-core@0.4.0"]["deps"]
            if dependency["pkg"] not in {gained, "arrow@57.3.0"}
        ]
        errors = notices.check(self.graph.metadata())[1]
        self.assertIn(
            "R/yamaa/inst/LICENSE-THIRD-PARTY.txt: lists arrow 57.3.0, which the "
            "locked graph no longer ships",
            errors,
        )
        self.assertIn(
            "R/yamaa/inst/LICENSE-THIRD-PARTY.txt: differs from the notices "
            "generated from rust/Cargo.lock",
            errors,
        )

    def test_package_copies_must_match_their_sources(self):
        metadata = self.graph.metadata()
        notices.write(metadata)
        (notices.R_INST / "LICENSE-YAML.txt").write_text("changed\n")
        (notices.R_INST / "LICENSE-WINDOWS.txt").write_text("stale\n")
        (notices.PYTHON / "LICENSE.txt").write_text("edited\n")
        self.project(
            license="MIT AND Apache-2.0 AND Unicode-3.0",
            files=("LICENSE.txt", "LICENSE-YAML.txt"),
        )
        errors = notices.check(metadata)[1]
        for expected in (
            "python/pyproject.toml: license-files must list LICENSE-THIRD-PARTY.txt",
            "R/yamaa/inst/LICENSE-YAML.txt: must be identical to python/LICENSE-YAML.txt",
            "R/yamaa/inst/LICENSE.txt: must be identical to python/LICENSE.txt",
            "R/yamaa/inst/LICENSE-WINDOWS.txt: not a package notice in license-files",
            "python/LICENSE-THIRD-PARTY.txt: not a package notice in license-files",
            "python/LICENSE.txt: must be a copy of LICENSE",
        ):
            self.assertIn(expected, errors)


if __name__ == "__main__":
    unittest.main()
