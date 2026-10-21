"""Third-party notices follow the locked closure of the shipped crates."""

import dataclasses
import re
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
# r-efi's AUTHORS states its three licenses, one of them a GNU license.
AUTHORS = (
    "LICENSE:\n    This project is triple-licensed under the MIT License, the "
    "Apache\n    License, Version 2.0, and the GNU Lesser General Public License,\n"
    "    Version 2.1+.\n\nAUTHORS-MIT:\n" + MIT.format(holder="r-efi")
)
# The clause of the LLVM exception that names GPLv2, as its texts wrap it.
LLVM_CLAUSE = (
    "In addition, if you combine or link compiled forms of this Software with\n"
    'software that is licensed under the GPLv2 ("Combined Software") and if a\n'
    "court of competent jurisdiction determines that the patent provision (Section\n"
    "3), the indemnity provision (Section 9) or other Section of the License\n"
    "conflicts with the conditions of the GPLv2, you may retroactively and\n"
    "prospectively choose to deem waived or otherwise exclude such Section(s) of\n"
    "the License, but only in their entirety and only with respect to the Combined\n"
    "Software.\n"
)
# The targets of the test graphs: a crate of any other target is not linked.
LINUX, WINDOWS = "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"
PROJECT_LICENSE = b"MIT License\n\nCopyright (c) 2026 yamaa contributors\n"
QUOTED = (
    'Copyright (c) 2016 Amanieu <a@example.org>\n\nTHE SOFTWARE IS PROVIDED "AS IS".\n'
)
# The shape of share/doc/rust/COPYRIGHT-library.html, with the template's
# whitespace around each notice text and its character references.
LIBRARY_HTML = """<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Copyright notices for The Rust Standard Library</title>
</head>
<body>

<h1>Copyright notices for The Rust Standard Library</h1>

<h2>Table of Contents</h2>
<ul>
    <li><a href="#short-version">Short version for non-lawyers</a></li>
</ul>

<h2 id="short-version">Short version for non-lawyers</h2>

The Rust Standard Library is dual-licensed under Apache 2.0 and MIT terms.

<h2 id="longer-version">Longer version</h2>

<p>Copyrights in the Rust Standard Library are retained by their
contributors.</p>

<p>It is licensed under the <a href="http://opensource.org/licenses/MIT">MIT</a>
license, at your option.</p>

<h2 id="in-tree-files">In-tree files</h2>

<p>The following licenses cover the in-tree source files:</p>

<div style="border:1px solid black; padding: 5px;">

    <p>
    <b>File/Directory:</b> <code>.</code>
    </p>

    <p><b>License:</b> Apache-2.0 OR MIT</p>

    <p><b>Copyright:</b> The Rust Project Developers</p>

    <p><b>Exceptions:</b></p>

<div style="border:1px solid black; padding: 5px;">
    <p>
    <b>File/Directory:</b> <code>library/std/src/sys/sync/mutex/fuchsia.rs</code>
    </p>

    <p><b>License:</b> BSD-2-Clause AND (Apache-2.0 OR MIT)</p>

    <p><b>Copyright:</b> 2016 The Fuchsia Authors</p>

</div>

</div>

<h2 id="out-of-tree-dependencies">Out-of-tree dependencies</h2>

<p>The following licenses cover the out-of-tree crates that were used in the
Rust Standard Library in this release:</p>

    <h3>\N{PACKAGE} hashbrown-0.17.1</h3>
    <p><b>URL:</b> <a href="https://crates.io/crates/hashbrown/0.17.1">https://crates.io/crates/hashbrown/0.17.1</a></p>
    <p><b>Authors:</b> Amanieu &#60;a@example.org&#62;</p>
    <p><b>License:</b> MIT OR Apache-2.0</p>

        <p><b>Notices:</b>

            <details>
                <summary><code>LICENSE-MIT</code></summary>
                <pre>
QUOTED
                </pre>
            </details>

        </p>

    <h3>\N{PACKAGE} vex-sdk-0.27.1</h3>
    <p><b>URL:</b> <a href="https://crates.io/crates/vex-sdk/0.27.1">https://crates.io/crates/vex-sdk/0.27.1</a></p>
    <p><b>Authors:</b> Tropical</p>
    <p><b>License:</b> MIT</p>


</body>
</html>
""".replace(
    "QUOTED",
    QUOTED.replace("<", "&#60;").replace(">", "&#62;").replace('"', "&#34;"),
)
# A parsed standard library notice whose crate shares a text with the graph.
LIBRARY = notices.StandardLibrary(
    release="1.99.1",
    statement=(
        "Copyrights in the Rust Standard Library are retained by their contributors.",
    ),
    files=(
        notices.LibraryFiles(
            path=".",
            license="Apache-2.0 OR MIT",
            copyrights=("The Rust Project Developers",),
            exceptions=(
                notices.LibraryFiles(
                    "library/core/src/unicode", "Unicode-3.0", ("Unicode, Inc",), ()
                ),
            ),
        ),
    ),
    licenses=(("MIT", MIT.format(holder="<year> <copyright holders>")),),
    crates=(
        notices.LibraryCrate(
            "windows-sys",
            "0.61.2",
            "MIT OR Apache-2.0",
            (("license-mit", MIT.format(holder="Microsoft")),),
        ),
        notices.LibraryCrate("vex-sdk", "0.27.1", "MIT", ()),
        notices.LibraryCrate(
            "r-efi",
            "5.3.0",
            "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
            (("AUTHORS", AUTHORS),),
        ),
    ),
)
# The standard library crates that the test library links.
LINKED = (("windows-sys", "0.61.2"),)


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
            "repository": options.get("repository"),
            "manifest_path": str(directory / "Cargo.toml"),
            "targets": [{"kind": [options.get("kind", "lib")]}],
        }
        self.nodes[identity] = {"id": identity, "deps": []}
        if member:
            self.members.append(identity)
        return identity

    def depend(self, parent, child, kind=None, targets=None):
        """Add an edge, on every target or only on `targets`."""
        self.nodes[parent]["deps"].append(
            {"pkg": child, "dep_kinds": [{"kind": kind, "targets": targets}]}
        )

    def metadata(self, target=None):
        """The graph, or as `cargo metadata --filter-platform target` gives it."""
        nodes = []
        for node in self.nodes.values():
            deps = []
            for dependency in node["deps"]:
                kinds = [
                    {"kind": kind["kind"], "target": None}
                    for kind in dependency["dep_kinds"]
                    if target is None
                    or kind["targets"] is None
                    or target in kind["targets"]
                ]
                if kinds:
                    deps.append({"pkg": dependency["pkg"], "dep_kinds": kinds})
            nodes.append({"id": node["id"], "deps": deps})
        return {
            "packages": list(self.packages.values()),
            "workspace_members": list(self.members),
            "resolve": {"nodes": nodes},
        }

    def graphs(self, targets=(LINUX, WINDOWS)):
        """The graph of each target, by target."""
        return {target: self.metadata(target) for target in targets}


def shipped_graph(root):
    """Two shipped roots with linked, build-only, macro-only, dev-only and
    other-platform crates; each root also links a crate that the other does
    not."""
    graph = Graph(root)
    python = graph.crate("yamaa-python", "0.4.0", "MIT", member=True)
    r = graph.crate("yamaa-r", "0.4.0", "MIT", member=True)
    core = graph.crate("yamaa-core", "0.4.0", "MIT AND Unicode-3.0", member=True)
    extendr = graph.crate(
        "extendr-api",
        "0.9.0",
        "BSD-3-Clause",
        {"LICENSE": BSD.replace("Example, Inc.", "extendr")},
    )
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
    efi = graph.crate(
        "r-efi",
        "5.3.0",
        "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
        {"AUTHORS": AUTHORS},
    )
    parser = graph.crate(
        "yamaa-saphyr-parser",
        "0.2.1",
        "MIT OR Apache-2.0",
        {"LICENSE": "Both authors' licenses are in LICENSES.\n"},
        source=None,
        directory=root / "rust/vendor/saphyr-parser",
    )
    for license in notices.EXTRA_FILES["yamaa-saphyr-parser"]:
        author = license.split("/")[1].split("-")[0]
        text = APACHE if license.endswith("Apache") else MIT.format(holder=author)
        (root / "rust/vendor/saphyr-parser" / license).parent.mkdir(exist_ok=True)
        (root / "rust/vendor/saphyr-parser" / license).write_text(text)
    graph.depend(python, core)
    graph.depend(python, pyo3)
    graph.depend(python, builder, kind="build")
    graph.depend(r, core)
    graph.depend(r, extendr)
    graph.depend(r, tester, kind="dev")
    graph.depend(core, arrow)
    graph.depend(core, derive)
    graph.depend(core, parser)
    graph.depend(arrow, flatbuffers)
    graph.depend(derive, keccak)
    graph.depend(pyo3, windows, targets={WINDOWS})
    graph.depend(core, efi, targets={"x86_64-unknown-uefi"})
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


class StandardLibraryNotice(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.documents = Path(temporary.name) / "share/doc/rust"
        (self.documents / "licenses").mkdir(parents=True)
        for license, text in (
            ("Apache-2.0", APACHE),
            ("BSD-2-Clause", "Copyright (c) <year> <owner>\n"),
            ("MIT", MIT.format(holder="<year> <copyright holders>")),
        ):
            (self.documents / "licenses" / f"{license}.txt").write_text(text)

    def read(self, html=LIBRARY_HTML):
        (self.documents / "COPYRIGHT-library.html").write_text(html, encoding="utf-8")
        return notices.read_standard_library(self.documents, "1.99.1")

    def test_reads_the_statement_tree_license_texts_and_crates(self):
        library = self.read()
        self.assertEqual(library.release, "1.99.1")
        self.assertEqual(
            library.statement,
            (
                "Copyrights in the Rust Standard Library are retained by their "
                "contributors.",
                "It is licensed under the MIT license, at your option.",
            ),
        )
        fuchsia = notices.LibraryFiles(
            "library/std/src/sys/sync/mutex/fuchsia.rs",
            "BSD-2-Clause AND (Apache-2.0 OR MIT)",
            ("2016 The Fuchsia Authors",),
            (),
        )
        self.assertEqual(
            library.files,
            (
                notices.LibraryFiles(
                    ".",
                    "Apache-2.0 OR MIT",
                    ("The Rust Project Developers",),
                    (fuchsia,),
                ),
            ),
        )
        # Every license an in-tree entry names, with the toolchain's text.
        self.assertEqual(
            [license for license, _ in library.licenses],
            ["Apache-2.0", "BSD-2-Clause", "MIT"],
        )
        # Notice texts come back verbatim from inside the template's wrapper.
        self.assertEqual(
            library.crates,
            (
                notices.LibraryCrate(
                    "hashbrown",
                    "0.17.1",
                    "MIT OR Apache-2.0",
                    (("LICENSE-MIT", QUOTED),),
                ),
                notices.LibraryCrate("vex-sdk", "0.27.1", "MIT", ()),
            ),
        )

    def test_another_shape_stops_generation(self):
        cases = {
            "expected a table of contents": LIBRARY_HTML.replace(
                '<h2 id="in-tree-files">', '<h2 id="in-tree">'
            ),
            "has 'Note: reviewed'": LIBRARY_HTML.replace(
                "<p><b>Copyright:</b> 2016", "<p><b>Note:</b> reviewed</p><p>2016"
            ),
            "has an unexpected text": LIBRARY_HTML.replace("<pre>\n", "<pre>"),
            "unexpected crate '\N{PACKAGE} vex-sdk-0.27.1'": LIBRARY_HTML.replace(
                "crates/vex-sdk/0.27.1", "crates/vex-sdk/0.27.2"
            ),
            "names no path or license": LIBRARY_HTML.replace(
                "<p><b>License:</b> Apache-2.0 OR MIT</p>", ""
            ),
        }
        for message, html in cases.items():
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, re.escape(message)):
                    self.read(html)
        (self.documents / "licenses/BSD-2-Clause.txt").unlink()
        with self.assertRaises(FileNotFoundError):
            self.read()

    def test_the_notice_comes_from_the_pinned_toolchain(self):
        self.read()
        sysroot = self.documents.parent.parent.parent
        with mock.patch.object(notices.toolchain, "channel", return_value="1.99.1"):
            with mock.patch.object(
                notices.subprocess,
                "check_output",
                side_effect=["rustc 1.99.1\nrelease: 1.99.1\n", f"{sysroot}\n"],
            ) as run:
                self.assertEqual(notices.standard_library(), self.read())
            self.assertEqual(
                [call.args[0] for call in run.call_args_list],
                [["rustc", "-vV"], ["rustc", "--print", "sysroot"]],
            )
            with mock.patch.object(
                notices.subprocess,
                "check_output",
                return_value="rustc 1.97.0\nrelease: 1.97.0\n",
            ):
                with self.assertRaisesRegex(
                    ValueError, "rustc is 1.97.0, but the notices need Rust 1.99.1"
                ):
                    notices.standard_library()


class GnuLicenses(unittest.TestCase):
    def test_names_and_abbreviations_are_found_apart_from_the_llvm_clause(self):
        self.assertEqual(
            notices.gnu_licenses(
                "under the GNU Lesser General\nPublic License, or LGPL-2.1-or-later; "
                "GPLv2 and AGPL too"
            ),
            ["GNU Lesser General Public License", "LGPL", "GPLv2", "AGPL"],
        )
        # The LLVM exception names GPLv2 but grants no GNU license; an
        # identifier such as `lgpl` is not a license.
        self.assertEqual(notices.gnu_licenses(APACHE + LLVM_CLAUSE), [])
        self.assertEqual(notices.gnu_licenses("pub lgpl: LOGPALETTE,\n"), [])
        self.assertEqual(
            notices.gnu_licenses(LLVM_CLAUSE.replace("In addition", "Also")),
            ["GPLv2", "GPLv2"],
        )
        with self.assertRaisesRegex(ValueError, r"AUTHORS: names a GNU license \(GNU"):
            notices.refuse_gnu(AUTHORS, "AUTHORS")
        self.assertEqual(notices.refuse_gnu(MIT, "MIT"), MIT)


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
            TARGETS=(LINUX, WINDOWS),
            LIBRARY_LINKED=LINKED,
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

    def test_the_notices_list_the_crates_linked_on_any_target(self):
        graphs = self.graph.graphs()
        # Neither binding links a build script's crate, a procedural macro or
        # the crates it uses, a dev-dependency, or r-efi, which only UEFI
        # links; windows-sys is linked on Windows alone.
        for crate, own in (
            ("yamaa-python", ["pyo3", "windows-sys"]),
            ("yamaa-r", ["extendr-api"]),
        ):
            with self.subTest(crate=crate):
                self.assertEqual(
                    [package["name"] for package in notices.third_party(graphs, crate)],
                    sorted(["arrow", "flatbuffers", "yamaa-saphyr-parser"] + own),
                )
        linux = notices.dependency_closure(graphs[LINUX], "yamaa-python")
        self.assertEqual(
            self.names(linux),
            {"yamaa-python", "yamaa-core", "arrow", "flatbuffers", "pyo3"}
            | {"yamaa-saphyr-parser"},
        )
        self.assertEqual(
            self.names(notices.linked_closure(graphs, "yamaa-python"))
            - self.names(linux),
            {"windows-sys"},
        )
        with self.assertRaisesRegex(ValueError, "the shipped crate yamaa-cli once"):
            notices.dependency_closure(graphs[LINUX], "yamaa-cli")

    def test_texts_follow_the_relied_license_and_fall_back_to_spdx(self):
        graphs = self.graph.graphs()
        selected = {
            notice.name: notice
            for notice in notices.notices_for(graphs, "yamaa-python")
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
        parser = selected["yamaa-saphyr-parser"]
        self.assertEqual(parser.source, "rust/vendor/saphyr-parser")
        self.assertEqual(
            [name for name, _ in parser.texts],
            ["LICENSE", "LICENSES/ChenYuheng-MIT", "LICENSES/Ethiraric-MIT"],
        )
        # The R binding's BSD-3-Clause crate is not linked into the wheels.
        self.assertEqual(
            notices.license_expression(graphs, selected.values()),
            "MIT AND Apache-2.0 AND Unicode-3.0",
        )
        rendered = notices.render(list(selected.values()), LIBRARY, "python")
        # Each entry names the one license relied on, never the choice; only
        # the Rust project's statement of its own files offers one.
        self.assertIn("Crate: pyo3 0.27.2\n    License: MIT\n    Texts: [", rendered)
        entries = re.findall(r"(?m)^ +License: (.*)$", rendered)
        self.assertEqual(
            [entry for entry in entries if " OR " in entry], ["Apache-2.0 OR MIT"]
        )
        # The build script's crate, which shares pyo3's text, is not listed.
        self.assertNotIn("cc 1.7.0", rendered)
        # The standard library's windows-sys is the locked one: one text, one name.
        self.assertEqual(rendered.count("Copyright (c) 2020 Microsoft"), 1)
        self.assertEqual(rendered.count("windows-sys 0.61.2 (license-mit)"), 1)
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
        # The standard library's crates are not locked crates.
        self.assertEqual(
            notices.committed_entries(rendered),
            {
                ("arrow", "57.3.0"),
                ("flatbuffers", "25.12.19"),
                ("pyo3", "0.27.2"),
                ("windows-sys", "0.61.2"),
                ("yamaa-saphyr-parser", "0.2.1"),
            },
        )
        self.assertEqual(notices.gnu_licenses(rendered), [])

    def test_a_linked_text_that_names_a_gnu_license_stops_generation(self):
        # A crate that the bindings link would bring r-efi's LGPL notice in.
        self.graph.depend("yamaa-core@0.4.0", "r-efi@5.3.0")
        graphs = self.graph.graphs()
        (efi,) = [
            notice
            for notice in notices.notices_for(graphs, "yamaa-r")
            if notice.name == "r-efi"
        ]
        self.assertEqual(efi.relied, ("MIT",))
        with self.assertRaisesRegex(
            ValueError,
            r"the r notices: names a GNU license \(GNU Lesser General Public License\)",
        ):
            notices.render([efi], LIBRARY, "r")
        with self.assertRaisesRegex(ValueError, "names a GNU license"):
            notices.write(graphs, LIBRARY)

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
        (notice,) = notices.notices_for(graph.graphs(), "yamaa-python")
        self.assertEqual(notice.relied, ("BSD-3-Clause",))
        self.assertEqual([name for name, _ in notice.texts], ["zstd/LICENSE"])
        rendered = notices.render([notice], LIBRARY, "python")
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
        parser = self.graph.packages["yamaa-saphyr-parser@0.2.1"]
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

    def test_relied_on_names_one_license_for_any_locked_package(self):
        metadata = self.graph.metadata()
        for identity, relied in (
            ("yamaa-core@0.4.0", "MIT AND Unicode-3.0"),
            ("pyo3@0.27.2", "MIT"),
            ("r-efi@5.3.0", "MIT"),
            ("cc@1.7.0", "MIT"),
            ("tiny-keccak@2.0.2", "CC0-1.0"),
            ("extendr-api@0.9.0", "BSD-3-Clause"),
        ):
            with self.subTest(identity=identity):
                self.assertEqual(
                    notices.relied_on(self.graph.packages[identity], metadata), relied
                )

    def test_shared_texts_come_from_a_crate_of_the_same_repository(self):
        graph = Graph(self.root / "shared")
        python = graph.crate("yamaa-python", "0.4.0", "MIT", member=True)
        graph.crate("yamaa-r", "0.4.0", "MIT", member=True)
        repository = "https://github.com/dropbox/rust-alloc-no-stdlib"
        lending = graph.crate(
            "alloc-no-stdlib",
            "2.0.4",
            "BSD-3-Clause",
            {"LICENSE": BSD.replace("Example", "Dropbox")},
            repository=repository,
        )
        borrowing = graph.crate(
            "alloc-stdlib", "0.2.4", "BSD-3-Clause", repository=repository
        )
        graph.depend(python, lending)
        graph.depend(python, borrowing)
        selected = {
            notice.name: notice
            for notice in notices.notices_for(graph.graphs(), "yamaa-python")
        }
        self.assertEqual(
            selected["alloc-stdlib"].texts,
            (("LICENSE of alloc-no-stdlib 2.0.4", BSD.replace("Example", "Dropbox")),),
        )
        self.assertEqual(selected["alloc-stdlib"].fallbacks, ())
        rendered = notices.render(list(selected.values()), LIBRARY, "python")
        self.assertIn("    Texts: [1] LICENSE of alloc-no-stdlib 2.0.4\n", rendered)
        self.assertIn(
            "[1] Shipped by: alloc-no-stdlib 2.0.4 (LICENSE),\n"
            "    alloc-stdlib 0.2.4 (LICENSE of alloc-no-stdlib 2.0.4)\n",
            rendered,
        )
        self.assertNotIn("<owner>", rendered)
        graph.packages[borrowing]["repository"] = "https://github.com/example/other"
        with self.assertRaisesRegex(ValueError, "must ship once, from the same"):
            notices.notices_for(graph.graphs(), "yamaa-python")
        graph.packages[borrowing]["repository"] = repository
        directory = Path(graph.packages[borrowing]["manifest_path"]).parent
        (directory / "LICENSE").write_text(BSD)
        with self.assertRaisesRegex(ValueError, "drop it from SHARED_TEXTS"):
            notices.notices_for(graph.graphs(), "yamaa-python")

    def test_the_standard_library_section_lists_only_the_crates_it_links(self):
        notice = notices.notices_for(self.graph.graphs(), "yamaa-python")
        rendered = notices.render(notice, LIBRARY, "python")
        crates, _, rest = rendered.partition(
            "\nRust standard library\n---------------------\n"
        )
        standard, _, texts = rest.partition("\nTexts\n-----\n")
        self.assertIn("Crate: yamaa-saphyr-parser 0.2.1", crates)
        self.assertIn(
            "From share/doc/rust/COPYRIGHT-library.html of Rust 1.99.1:\n\n"
            "Copyrights in the Rust Standard Library are retained by their "
            "contributors.\n\n"
            "File/Directory: .\n"
            "    License: Apache-2.0 OR MIT\n"
            "    Copyright: The Rust Project Developers\n"
            "    Exceptions:\n"
            "        File/Directory: library/core/src/unicode\n"
            "            License: Unicode-3.0\n"
            "            Copyright: Unicode, Inc\n",
            standard,
        )
        # Of the notice's crates, the section keeps those that LIBRARY_LINKED
        # names, each with the one license relied on.
        self.assertIn(
            "Crate: windows-sys 0.61.2\n    License: MIT\n    Texts: [", standard
        )
        self.assertNotIn("vex-sdk", standard)
        self.assertNotIn("r-efi", rendered)
        self.assertIn(
            "Shipped by: Rust 1.99.1 (share/doc/rust/licenses/MIT.txt)\n", texts
        )
        self.assertIn("from\nrust/Cargo.lock and the Rust 1.99.1 toolchain", rendered)
        # A linked crate whose texts the notice leaves out takes the canonical
        # text; one that the notice does not name needs the table reviewed.
        with mock.patch.object(notices, "LIBRARY_LINKED", (("vex-sdk", "0.27.1"),)):
            rendered = notices.render(notice, LIBRARY, "python")
        self.assertIn("Crate: vex-sdk 0.27.1\n    License: MIT\n    Texts: [", rendered)
        self.assertIn(
            "canonical MIT text; the Rust project's notice reproduces none", rendered
        )
        with mock.patch.object(notices, "LIBRARY_LINKED", (("libc", "0.2.189"),)):
            with self.assertRaisesRegex(
                ValueError,
                "libc 0.2.189 of the Rust 1.99.1 standard library: its notice "
                "does not name it; review LIBRARY_LINKED",
            ):
                notices.render(notice, LIBRARY, "python")

    def test_check_fails_when_the_closure_gains_or_loses_a_crate(self):
        graphs = self.graph.graphs()
        notices.write(graphs, LIBRARY)
        self.assertEqual(notices.check(graphs, LIBRARY)[1], [])
        self.assertEqual((notices.PYTHON / "LICENSE.txt").read_bytes(), PROJECT_LICENSE)
        # A toolchain whose notice changed needs the notices regenerated.
        changed = dataclasses.replace(
            LIBRARY, statement=("Copyrights are retained by their contributors.",)
        )
        self.assertEqual(
            notices.check(graphs, changed)[1],
            [
                f"{relative}/LICENSE-THIRD-PARTY.txt: differs from the notices "
                f"generated for {crate} from rust/Cargo.lock and the Rust 1.99.1 "
                "toolchain"
                for relative, crate in (
                    ("python", "yamaa-python"),
                    ("R/yamaa/inst", "yamaa-r"),
                )
            ],
        )
        gained = self.graph.crate("snap", "1.1.2", "BSD-3-Clause", {"COPYING": BSD})
        self.graph.depend("yamaa-core@0.4.0", gained)
        errors = notices.check(self.graph.graphs(), LIBRARY)[1]
        for relative, crate in (
            ("python", "yamaa-python"),
            ("R/yamaa/inst", "yamaa-r"),
        ):
            self.assertIn(
                f"{relative}/LICENSE-THIRD-PARTY.txt: no entry for snap 1.1.2, which "
                f"{crate} links",
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
        errors = notices.check(self.graph.graphs(), LIBRARY)[1]
        self.assertIn(
            "R/yamaa/inst/LICENSE-THIRD-PARTY.txt: lists arrow 57.3.0, which "
            "yamaa-r does not link",
            errors,
        )
        self.assertIn(
            "R/yamaa/inst/LICENSE-THIRD-PARTY.txt: differs from the notices "
            "generated for yamaa-r from rust/Cargo.lock and the Rust 1.99.1 "
            "toolchain",
            errors,
        )

    def test_each_host_lists_only_the_crates_of_its_binding(self):
        graphs = self.graph.graphs()
        selected = notices.write(graphs, LIBRARY)
        python = (notices.PYTHON / "LICENSE-THIRD-PARTY.txt").read_text()
        r = (notices.R_INST / "LICENSE-THIRD-PARTY.txt").read_text()
        shared = {
            ("arrow", "57.3.0"),
            ("flatbuffers", "25.12.19"),
            ("yamaa-saphyr-parser", "0.2.1"),
        }
        python_only = {("pyo3", "0.27.2"), ("windows-sys", "0.61.2")}
        self.assertEqual(notices.committed_entries(python), shared | python_only)
        self.assertEqual(
            notices.committed_entries(r), shared | {("extendr-api", "0.9.0")}
        )
        self.assertEqual(
            {host: len(found) for host, found in selected.items()},
            {"python": 5, "r": 4},
        )
        for text, packages, crate in (
            (python, "The yamaa Python wheels link", "yamaa-python"),
            (r, "The installed yamaa R package links", "yamaa-r"),
        ):
            flat = " ".join(text.split())
            self.assertIn(f"{packages} third-party Rust crates", flat)
            self.assertIn(
                f"crates that the shipped crate {crate} links, as locked in "
                "rust/Cargo.lock: its normal dependency closure on each of the "
                f"targets {LINUX}, {WINDOWS}, without procedural macros.",
                flat,
            )
            # Both hosts link the standard library and list its linked crates.
            self.assertIn("Crate: windows-sys 0.61.2\n    License: MIT\n", text)
        # Only the copy whose crates ship a text reproduces it.
        self.assertNotIn("Copyright (c) 2020 PyO3", r)
        self.assertNotIn("Copyright (c) 2016 extendr", python)
        # An R package copy that names the Python binding's crates is stale.
        (notices.R_INST / "LICENSE-THIRD-PARTY.txt").write_text(python)
        lists = (
            "R/yamaa/inst/LICENSE-THIRD-PARTY.txt: lists {}, which yamaa-r does "
            "not link"
        )
        self.assertEqual(
            notices.check(graphs, LIBRARY)[1],
            [
                "R/yamaa/inst/LICENSE-THIRD-PARTY.txt: no entry for extendr-api "
                "0.9.0, which yamaa-r links",
                lists.format("pyo3 0.27.2"),
                lists.format("windows-sys 0.61.2"),
                "R/yamaa/inst/LICENSE-THIRD-PARTY.txt: differs from the notices "
                "generated for yamaa-r from rust/Cargo.lock and the Rust 1.99.1 "
                "toolchain",
            ],
        )

    def test_package_copies_must_match_their_sources(self):
        graphs = self.graph.graphs()
        notices.write(graphs, LIBRARY)
        (notices.R_INST / "LICENSE-YAML.txt").write_text("changed\n")
        (notices.R_INST / "LICENSE-WINDOWS.txt").write_text("stale\n")
        (notices.PYTHON / "LICENSE.txt").write_text("edited\n")
        self.project(
            license="MIT AND Apache-2.0 AND Unicode-3.0",
            files=("LICENSE.txt", "LICENSE-YAML.txt"),
        )
        errors = notices.check(graphs, LIBRARY)[1]
        for expected in (
            "python/pyproject.toml: license-files must list LICENSE-THIRD-PARTY.txt",
            "R/yamaa/inst/LICENSE-YAML.txt: must be identical to python/LICENSE-YAML.txt",
            "R/yamaa/inst/LICENSE.txt: must be identical to python/LICENSE.txt",
            "R/yamaa/inst/LICENSE-WINDOWS.txt: not a package notice in license-files",
            "python/LICENSE-THIRD-PARTY.txt: not a package notice in license-files",
            "python/LICENSE.txt: must be a copy of LICENSE",
        ):
            self.assertIn(expected, errors)

    def test_only_the_notices_and_the_component_inventory_differ_between_hosts(self):
        # Each host's packages carry the notices and the inventory of their own
        # crates; tools.packaging.generate_component_inventory checks the
        # inventories.
        graphs = self.graph.graphs()
        notices.write(graphs, LIBRARY)
        self.assertNotEqual(
            (notices.PYTHON / "LICENSE-THIRD-PARTY.txt").read_bytes(),
            (notices.R_INST / "LICENSE-THIRD-PARTY.txt").read_bytes(),
        )
        self.project(
            license="MIT AND Apache-2.0 AND Unicode-3.0",
            files=(
                "LICENSE.txt",
                "LICENSE-THIRD-PARTY.txt",
                "LICENSE-YAML.txt",
                notices.INVENTORY,
            ),
        )
        self.assertEqual(
            notices.check(graphs, LIBRARY)[1],
            ["python/COMPONENTS.tsv: listed in license-files but missing"],
        )
        (notices.PYTHON / notices.INVENTORY).write_text("name\npyo3\n")
        (notices.R_INST / notices.INVENTORY).write_text("name\nextendr-api\n")
        self.assertEqual(notices.check(graphs, LIBRARY)[1], [])
        (notices.R_INST / "LICENSE-YAML.txt").write_text("changed\n")
        self.assertEqual(
            notices.check(graphs, LIBRARY)[1],
            [
                "R/yamaa/inst/LICENSE-YAML.txt: must be identical to "
                "python/LICENSE-YAML.txt"
            ],
        )


if __name__ == "__main__":
    unittest.main()
