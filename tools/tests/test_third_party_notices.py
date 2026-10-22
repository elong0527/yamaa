"""Third-party notices follow the locked closure of the shipped crates."""

import dataclasses
import os
import re
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.packaging import generate_third_party_notices as notices
from tools.staging import engine_bundle

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
# compiler-builtins' LICENSE.txt: its statement, MIT, and Apache-2.0 with the
# LLVM exception (#3494).
BUILTINS = (
    "The compiler-builtins crate is available for use under both the MIT "
    "license\nand the Apache-2.0 license with the LLVM exception.\n\n"
    + MIT.format(holder="compiler-builtins")
    + APACHE
    + "---- LLVM Exceptions to the Apache 2.0 License ----\n\n"
    + LLVM_CLAUSE
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
# A standard library whose linked crate shares a text with the graph.
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
        notices.Notice(
            "windows-sys",
            "0.61.2",
            None,
            ("MIT",),
            (("license-mit", MIT.format(holder="Microsoft")),),
            (),
        ),
    ),
)
# The command line's musl notice: the header, then musl's COPYRIGHT file.
MUSL_NOTICE = (
    f"musl {notices.MUSL[1]}\n{notices.MUSL_HOME}\n\nmusl as a whole is licensed "
    "under the following standard MIT license:\n\n" + MIT.format(holder="Rich Felker")
)
LINUX_OPTIONS = frozenset(
    {
        "unix",
        ("target_os", "linux"),
        ("target_env", "gnu"),
        ("target_family", "unix"),
        ("target_vendor", "unknown"),
    }
)
WINDOWS_OPTIONS = frozenset(
    {
        "windows",
        ("target_os", "windows"),
        ("target_env", "msvc"),
        ("target_family", "windows"),
        ("target_vendor", "pc"),
    }
)
# The platforms of the test standard library's sources.
PLATFORMS = (
    notices.Platform(LINUX, LINUX_OPTIONS),
    notices.Platform(WINDOWS, WINDOWS_OPTIONS),
)
# What `rustc --print cfg --target` prints for each test platform.
PRINTED_CFG = {
    LINUX: 'debug_assertions\ntarget_env="gnu"\ntarget_family="unix"\n'
    'target_os="linux"\ntarget_vendor="unknown"\nunix\n',
    WINDOWS: 'debug_assertions\ntarget_env="msvc"\ntarget_family="windows"\n'
    'target_os="windows"\ntarget_vendor="pc"\nwindows\n',
}


def standard_library_sources(root):
    """Write a rust-src library directory: std's workspace, its lock and the
    packages in vendor/ of its out-of-tree crates.

    std links cfg-if and hashbrown everywhere, libc off Windows, and with its
    backtrace feature addr2line and gimli. hashbrown asks for foldhash only
    through a weak feature, unwinding and dlmalloc are xous's, rand is a
    dev-dependency and cc a build dependency. Of the in-tree crates, std links
    compiler_builtins everywhere, which declares a license of its own.
    """
    library = root / "library"
    registry = '\nsource = "registry+https://github.com/rust-lang/crates.io-index"'

    def package(name, version, dependencies=(), source=True):
        listed = ", ".join(f'"{dependency}"' for dependency in dependencies)
        return (
            f'[[package]]\nname = "{name}"\nversion = "{version}"'
            f"{registry if source else ''}\ndependencies = [{listed}]\n"
        )

    core = "rustc-std-workspace-core"
    lock = "version = 4\n\n" + "\n".join(
        [
            package(
                "std",
                "0.0.0",
                ["addr2line", "cc", "cfg-if", "compiler_builtins", "core"]
                + ["dlmalloc", "hashbrown", "libc", "rand", "unwind"],
                source=False,
            ),
            package("compiler_builtins", "0.1.160", ["core"], source=False),
            package("core", "0.0.0", source=False),
            package(core, "1.99.0", ["core"], source=False),
            package("unwind", "0.0.0", ["libc", core, "unwinding"], source=False),
            package("addr2line", "0.27.1", ["gimli", core]),
            package("cc", "1.2.0"),
            package("cfg-if", "1.0.4", [core]),
            package("dlmalloc", "0.2.14", [core]),
            package("foldhash", "0.2.0"),
            package("gimli", "0.34.0", [core]),
            package("hashbrown", "0.17.1", ["foldhash", core]),
            package("libc", "0.2.189", [core]),
            package("rand", "0.9.5"),
            package("unwinding", "0.2.10", ["gimli", core]),
        ]
    )
    manifests = {
        "Cargo.toml": (
            '[workspace]\nresolver = "1"\nmembers = ["std"]\n\n[patch.crates-io]\n'
            f"{core} = {{ path = '{core}' }}\n"
        ),
        "std/Cargo.toml": """[package]
name = "std"
version = "0.0.0"
license = "MIT OR Apache-2.0"

[dependencies]
core = { path = "../core" }
compiler_builtins = { path = "../compiler-builtins/compiler-builtins" }
cfg-if = { version = "1.0", features = ['rustc-dep-of-std'] }
hashbrown = { version = "0.17", default-features = false, features = [
    'rustc-dep-of-std',
] }
unwind = { path = "../unwind" }

[target.'cfg(not(all(windows, target_env = "msvc")))'.dependencies]
libc = { version = "0.2", default-features = false }
addr2line = { version = "0.27", optional = true, default-features = false }

[target.'cfg(target_os = "xous")'.dependencies]
dlmalloc = "0.2"

[target.x86_64-fortanix-unknown-sgx.dependencies]
dlmalloc = "0.2"

[dev-dependencies]
rand = "0.9"

[build-dependencies]
cc = "1.2"

[features]
backtrace = ['addr2line/rustc-dep-of-std']
panic-unwind = []
""",
        "core/Cargo.toml": '[package]\nname = "core"\nversion = "0.0.0"\n',
        "compiler-builtins/compiler-builtins/Cargo.toml": (
            '[package]\nname = "compiler_builtins"\nversion = "0.1.160"\n'
            'license = "MIT AND Apache-2.0 WITH LLVM-exception AND (MIT OR '
            'Apache-2.0)"\n\n[dependencies]\ncore = { path = "../../core" }\n'
        ),
        "compiler-builtins/LICENSE.txt": BUILTINS,
        "compiler-builtins/libm/LICENSE.txt": MIT.format(holder="libm"),
        f"{core}/Cargo.toml": (
            f'[package]\nname = "{core}"\nversion = "1.99.0"\n\n'
            '[dependencies]\ncore = { path = "../core" }\n'
        ),
        "unwind/Cargo.toml": f"""[package]
name = "unwind"
version = "0.0.0"

[dependencies]
core = {{ path = "../{core}", package = "{core}" }}

[target.'cfg(not(all(windows, target_env = "msvc")))'.dependencies]
libc = {{ version = "0.2", default-features = false }}

[target.'cfg(target_os = "xous")'.dependencies]
unwinding = {{ version = "0.2", default-features = false }}
""",
    }
    # A vendored crate's manifest, as `cargo publish` normalizes it, with the
    # optional core dependency that its rustc-dep-of-std feature enables.
    vendored = {
        "addr2line-0.27.1": (
            "MIT OR Apache-2.0",
            '[dependencies.gimli]\nversion = "0.34"\ndefault-features = false\n'
            '[features]\nrustc-dep-of-std = ["core", "gimli/rustc-dep-of-std"]\n',
            {"LICENSE-MIT": MIT.format(holder="addr2line"), "LICENSE-APACHE": APACHE},
        ),
        "cc-1.2.0": ("MIT OR Apache-2.0", "", {"LICENSE-MIT": MIT.format(holder="cc")}),
        "cfg-if-1.0.4": (
            "MIT OR Apache-2.0",
            '[features]\nrustc-dep-of-std = ["core"]\n',
            {"LICENSE-MIT": MIT.format(holder="Alex Crichton")},
        ),
        "dlmalloc-0.2.14": ("MIT OR Apache-2.0", "", {}),
        "foldhash-0.2.0": (
            "BSD-3-Clause",
            "[features]\nnightly = []\n",
            {"LICENSE": BSD},
        ),
        "gimli-0.34.0": (
            "MIT OR Apache-2.0",
            '[features]\nrustc-dep-of-std = ["core"]\n',
            {"LICENSE-MIT": MIT.format(holder="gimli")},
        ),
        "hashbrown-0.17.1": (
            "MIT OR Apache-2.0",
            '[dependencies.foldhash]\nversion = "0.2.0"\noptional = true\n'
            "[features]\n"
            'default = ["default-hasher"]\ndefault-hasher = ["dep:foldhash"]\n'
            'nightly = ["foldhash?/nightly"]\n'
            'rustc-dep-of-std = ["nightly", "core"]\n',
            {"LICENSE-MIT": MIT.format(holder="Amanieu")},
        ),
        "libc-0.2.189": (
            "MIT OR Apache-2.0",
            "[features]\ndefault = []\n",
            {"LICENSE-MIT": MIT.format(holder="libc")},
        ),
        "rand-0.9.5": ("MIT OR Apache-2.0", "", {}),
        "unwinding-0.2.10": (
            "MIT OR Apache-2.0",
            '[dependencies.gimli]\nversion = "0.34"\n',
            {},
        ),
    }
    for relative, text in manifests.items():
        (library / relative).parent.mkdir(parents=True, exist_ok=True)
        (library / relative).write_text(text)
    (library / "Cargo.lock").write_text(lock)
    for directory, (license, tables, files) in vendored.items():
        name, _, version = directory.rpartition("-")
        crate = library / "vendor" / directory
        crate.mkdir(parents=True)
        optional_core = (
            f'[dependencies.core]\nversion = "1.0.0"\noptional = true\n'
            f'package = "{core}"\n'
        )
        (crate / "Cargo.toml").write_text(
            f'[package]\nname = "{name}"\nversion = "{version}"\n'
            f'license = "{license}"\n{optional_core}{tables}'
        )
        for relative, text in files.items():
            (crate / relative).write_text(text)
    return library


class Graph:
    """Build `cargo metadata` output over crate directories in a temporary
    root, and the `cargo tree` output of each build of a shipped crate."""

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

    def metadata(self):
        """The packages and the workspace members, as `cargo metadata` gives
        them."""
        return {
            "packages": list(self.packages.values()),
            "workspace_members": list(self.members),
        }

    def printed(self, command, **options):
        """What `cargo tree` prints for the package and target of a command:
        that member's normal dependency closure on that target, without
        procedural macros, a repeated package marked (*)."""
        crate = command[command.index("--package") + 1]
        target = command[command.index("--target") + 1]
        roots = [
            identity
            for identity in self.members
            if self.packages[identity]["name"] == crate
        ]
        if not roots:
            raise subprocess.CalledProcessError(101, command)
        seen, pending, lines = set(), roots, []
        while pending:
            identity = pending.pop()
            package = self.packages[identity]
            line = f"{package['name']} v{package['version']}"
            if package["source"] != notices.CRATES_IO:
                line += f" ({Path(package['manifest_path']).parent})"
            if identity in seen:
                lines.append(f"{line} (*)")
                continue
            seen.add(identity)
            lines.append(line)
            for dependency in self.nodes[identity]["deps"]:
                child = self.packages[dependency["pkg"]]
                if any(
                    kind["kind"] is None
                    and (kind["targets"] is None or target in kind["targets"])
                    for kind in dependency["dep_kinds"]
                ) and child["targets"][0]["kind"] != ["proc-macro"]:
                    pending.append(dependency["pkg"])
        return "\n".join(lines) + "\n"

    def closures(self):
        """The IDs of the packages that each host's shipped crate links, as
        linked_packages reads them from what `cargo tree` prints."""
        with mock.patch.object(
            notices.subprocess, "check_output", side_effect=self.printed
        ):
            return notices.linked_packages(self.metadata())


def shipped_graph(root):
    """Two shipped bindings with linked, build-only, macro-only, dev-only and
    other-platform crates, each also linking a crate that the other does not,
    and the command line, which links the engine's crates alone."""
    graph = Graph(root)
    python = graph.crate("yamaa-python", "0.4.0", "MIT", member=True)
    r = graph.crate("yamaa-r", "0.4.0", "MIT", member=True)
    cli = graph.crate("yamaa-cli", "0.4.0", "MIT", member=True)
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
    graph.depend(cli, core)
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
        self.sysroot = Path(temporary.name)
        self.documents = self.sysroot / "share/doc/rust"
        (self.documents / "licenses").mkdir(parents=True)
        for license, text in (
            ("Apache-2.0", APACHE),
            ("BSD-2-Clause", "Copyright (c) <year> <owner>\n"),
            ("MIT", MIT.format(holder="<year> <copyright holders>")),
        ):
            (self.documents / "licenses" / f"{license}.txt").write_text(text)

    def read(self, html=LIBRARY_HTML):
        (self.documents / "COPYRIGHT-library.html").write_text(html, encoding="utf-8")
        return notices.read_library_notice(self.documents)

    def test_reads_the_statement_tree_and_license_texts(self):
        statement, files, licenses = self.read()
        self.assertEqual(
            statement,
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
            files,
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
            [license for license, _ in licenses],
            ["Apache-2.0", "BSD-2-Clause", "MIT"],
        )
        # The out-of-tree section, which misses crates that std links, is not
        # read: the crates come from the standard library's sources.
        unread = LIBRARY_HTML.replace("<pre>\n", "<pre>").replace(
            "crates/vex-sdk/0.27.1", "crates/vex-sdk/0.27.2"
        )
        self.assertEqual(self.read(unread), (statement, files, licenses))

    def test_another_shape_stops_generation(self):
        cases = {
            "expected a table of contents": LIBRARY_HTML.replace(
                '<h2 id="in-tree-files">', '<h2 id="in-tree">'
            ),
            "has 'Note: reviewed'": LIBRARY_HTML.replace(
                "<p><b>Copyright:</b> 2016", "<p><b>Note:</b> reviewed</p><p>2016"
            ),
            "names no path or license": LIBRARY_HTML.replace(
                "<p><b>License:</b> Apache-2.0 OR MIT</p>", ""
            ),
            "names no statement or files": LIBRARY_HTML.replace(
                "<p>Copyrights in the Rust Standard Library are retained by their\n"
                "contributors.</p>",
                "",
            ).replace(
                '<p>It is licensed under the <a href="http://opensource.org/licenses/'
                'MIT">MIT</a>\nlicense, at your option.</p>',
                "",
            ),
        }
        for message, html in cases.items():
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, re.escape(message)):
                    self.read(html)
        (self.documents / "licenses/BSD-2-Clause.txt").unlink()
        with self.assertRaises(FileNotFoundError):
            self.read()

    def test_the_library_comes_from_the_pinned_toolchain(self):
        # The notice and the rust-src sources below the pinned toolchain's
        # sysroot, and the crates that std links on each of the TARGETS.
        self.read()
        sources = standard_library_sources(self.sysroot / "lib/rustlib/src/rust")
        expected = notices.read_standard_library(
            self.documents, sources, "1.99.1", PLATFORMS
        )
        self.assertEqual(
            [crate.name for crate in expected.crates],
            ["addr2line", "cfg-if", "compiler_builtins", "gimli", "hashbrown"]
            + ["libc"],
        )
        printed = [
            "rustc 1.99.1\nrelease: 1.99.1\n",
            f"{self.sysroot}\n",
            PRINTED_CFG[LINUX],
            PRINTED_CFG[WINDOWS],
        ]
        with (
            mock.patch.object(notices, "TARGETS", (LINUX, WINDOWS)),
            mock.patch.object(notices.toolchain, "channel", return_value="1.99.1"),
            mock.patch.dict(os.environ),
        ):
            os.environ.pop(notices.LIBRARY_SOURCES_VARIABLE, None)
            with mock.patch.object(
                notices.subprocess, "check_output", side_effect=printed
            ) as run:
                self.assertEqual(notices.standard_library(), expected)
            self.assertEqual(
                [call.args[0] for call in run.call_args_list],
                [
                    ["rustc", "-vV"],
                    ["rustc", "--print", "sysroot"],
                    ["rustc", "--print", "cfg", "--target", LINUX],
                    ["rustc", "--print", "cfg", "--target", WINDOWS],
                ],
            )
            # A toolchain without rust-src takes an unpacked copy of it.
            unpacked = self.sysroot / "rust-src/library"
            unpacked.parent.mkdir()
            sources.rename(unpacked)
            with mock.patch.object(
                notices.subprocess, "check_output", side_effect=printed
            ):
                with self.assertRaisesRegex(
                    ValueError,
                    "holds no standard library sources; add the rust-src component "
                    "to Rust 1.99.1",
                ):
                    notices.standard_library()
            os.environ[notices.LIBRARY_SOURCES_VARIABLE] = str(unpacked)
            with mock.patch.object(
                notices.subprocess, "check_output", side_effect=printed
            ):
                self.assertEqual(notices.standard_library(), expected)
            with mock.patch.object(
                notices.subprocess,
                "check_output",
                return_value="rustc 1.97.0\nrelease: 1.97.0\n",
            ):
                with self.assertRaisesRegex(
                    ValueError, "rustc is 1.97.0, but the notices need Rust 1.99.1"
                ):
                    notices.standard_library()


class StandardLibrarySources(unittest.TestCase):
    """The crates that std links come from a rust-src library directory."""

    def sources(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        return standard_library_sources(Path(temporary.name))

    def linked(self, sources, platforms=PLATFORMS):
        return [crate.name for crate in notices.library_crates(sources, platforms)]

    def test_std_links_the_dependencies_of_each_platform_and_feature(self):
        sources = self.sources()
        crates = notices.library_crates(sources, PLATFORMS)
        # Neither the crate that a weak feature names, nor xous's or sgx's
        # crates, nor a dev-dependency or a build dependency.
        self.assertEqual(
            [(crate.name, crate.version) for crate in crates],
            [
                ("addr2line", "0.27.1"),
                ("cfg-if", "1.0.4"),
                ("compiler_builtins", "0.1.160"),
                ("gimli", "0.34.0"),
                ("hashbrown", "0.17.1"),
                ("libc", "0.2.189"),
            ],
        )
        # Each crate is chosen like a locked one, from its package in vendor/.
        self.assertEqual(
            crates[0],
            notices.Notice(
                "addr2line",
                "0.27.1",
                None,
                ("MIT",),
                (("LICENSE-MIT", MIT.format(holder="addr2line")),),
                (),
            ),
        )
        # An in-tree crate that declares a license of its own is chosen like a
        # crate from the files that LIBRARY_IN_TREE names (#3494): every part
        # of MIT AND Apache-2.0 WITH LLVM-exception AND (MIT OR Apache-2.0).
        self.assertEqual(
            crates[2],
            notices.Notice(
                "compiler_builtins",
                "0.1.160",
                "in-tree, compiler-builtins/compiler-builtins",
                ("MIT", "Apache-2.0 WITH LLVM-exception"),
                (
                    ("compiler-builtins/LICENSE.txt", BUILTINS),
                    ("compiler-builtins/libm/LICENSE.txt", MIT.format(holder="libm")),
                ),
                (),
            ),
        )
        # Windows with MSVC links neither libc nor the backtrace crates.
        self.assertEqual(
            self.linked(sources, PLATFORMS[1:]),
            ["cfg-if", "compiler_builtins", "hashbrown"],
        )
        # Without backtrace, std links no crate that symbolizes backtraces.
        with mock.patch.object(notices, "LIBRARY_FEATURES", ("panic-unwind",)):
            self.assertEqual(
                self.linked(sources),
                ["cfg-if", "compiler_builtins", "hashbrown", "libc"],
            )

    def test_in_tree_crates_of_their_own_license_are_the_reviewed_ones(self):
        # Another license than the notice's, unreviewed, stops generation, as
        # do a reviewed crate that std does not link and one that declares
        # the notice's license.
        builtins = "compiler-builtins/compiler-builtins/Cargo.toml"
        for message, table, edit in (
            (
                "compiler_builtins 0.1.160: the in-tree crate declares 'MIT AND "
                "Apache-2.0 WITH LLVM-exception AND (MIT OR Apache-2.0)'",
                {},
                None,
            ),
            (
                "LIBRARY_IN_TREE lists profiler_builtins, which std does not link",
                dict(notices.LIBRARY_IN_TREE, profiler_builtins=()),
                None,
            ),
            (
                "compiler_builtins 0.1.160: the in-tree crate declares "
                "'Apache-2.0 OR MIT'",
                notices.LIBRARY_IN_TREE,
                (
                    builtins,
                    "MIT AND Apache-2.0 WITH LLVM-exception AND (MIT OR Apache-2.0)",
                    "Apache-2.0 OR MIT",
                ),
            ),
        ):
            sources = self.sources()
            if edit:
                relative, old, new = edit
                manifest = sources / relative
                manifest.write_text(manifest.read_text().replace(old, new))
            with (
                self.subTest(message=message),
                mock.patch.object(notices, "LIBRARY_IN_TREE", table),
                self.assertRaisesRegex(ValueError, re.escape(message)),
            ):
                notices.library_crates(sources, PLATFORMS)

    def test_a_feature_enables_an_optional_dependency_and_a_weak_one_does_not(self):
        sources = self.sources()
        # hashbrown's default features enable foldhash, which std leaves out.
        manifest = sources / "std/Cargo.toml"
        manifest.write_text(
            manifest.read_text().replace(
                "default-features = false, features", "features"
            )
        )
        self.assertIn("foldhash", self.linked(sources))

    def test_sources_that_do_not_settle_a_crate_stop_generation(self):
        def change(relative, old, new):
            def apply(sources):
                path = sources / relative
                path.write_text(path.read_text().replace(old, new, 1))

            return apply

        for message, edit in (
            (
                "addr2line 0.27.1: library/Cargo.lock locks no one gimli for it",
                change("Cargo.lock", '"gimli", ', ""),
            ),
            (
                "vendor/libc-0.2.189: does not hold libc 0.2.189, which "
                "library/Cargo.lock locks",
                change("vendor/libc-0.2.189/Cargo.toml", "0.2.189", "0.2.190"),
            ),
            (
                "cfg-if 1.0.4: declares no feature std",
                change("std/Cargo.toml", "'rustc-dep-of-std'] }", "'std'] }"),
            ),
            (
                "malformed cfg expression: 'cfg(target_os = xous)'",
                change("std/Cargo.toml", 'target_os = "xous"', "target_os = xous"),
            ),
            (
                "hashbrown 0.17.1: review the license 'GPL-3.0-only' (GPL-3.0-only) "
                "(the standard library's lib/rustlib/src/rust/library)",
                change(
                    "vendor/hashbrown-0.17.1/Cargo.toml",
                    "MIT OR Apache-2.0",
                    "GPL-3.0-only",
                ),
            ),
        ):
            sources = self.sources()
            edit(sources)
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, re.escape(message)):
                    notices.library_crates(sources, PLATFORMS)
        sources = self.sources()
        for path in sorted((sources / "vendor/gimli-0.34.0").iterdir()):
            path.unlink()
        with self.assertRaises(FileNotFoundError):
            notices.library_crates(sources, PLATFORMS)


class TargetConfiguration(unittest.TestCase):
    def test_a_target_table_applies_by_its_name_or_cfg_expression(self):
        linux, windows = PLATFORMS
        for specification, on_linux, on_windows in (
            (LINUX, True, False),
            ("x86_64-fortanix-unknown-sgx", False, False),
            ("cfg(unix)", True, False),
            ("cfg(windows)", False, True),
            ('cfg(not(all(windows, target_env = "msvc")))', True, False),
            (
                'cfg(not(all(windows, target_env = "msvc", not(target_vendor = '
                '"uwp"))))',
                True,
                False,
            ),
            ('cfg(any(target_os = "linux", target_os = "android",))', True, False),
            ('cfg(target_os="windows")', False, True),
            ("cfg(any())", False, False),
            ("cfg(all())", True, True),
        ):
            with self.subTest(specification=specification):
                self.assertEqual(
                    (
                        notices.applies(specification, linux),
                        notices.applies(specification, windows),
                    ),
                    (on_linux, on_windows),
                )
        for specification in (
            "cfg(unix",
            "cfg()",
            "cfg(unix) extra",
            "cfg(unix)!",
            "cfg(not(unix, windows))",
            "cfg(all(unix windows))",
            "cfg(target_os = linux)",
        ):
            with (
                self.subTest(specification=specification),
                self.assertRaisesRegex(ValueError, "malformed cfg expression"),
            ):
                notices.applies(specification, linux)

    def test_the_options_come_from_rustc_for_the_target(self):
        with mock.patch.object(
            notices.subprocess,
            "check_output",
            return_value=PRINTED_CFG[LINUX] + 'target_abi=""\n',
        ) as run:
            platform = notices.target_platform(LINUX)
        run.assert_called_once_with(
            ["rustc", "--print", "cfg", "--target", LINUX],
            cwd=notices.WORKSPACE,
            text=True,
        )
        self.assertEqual(
            platform,
            notices.Platform(
                LINUX, LINUX_OPTIONS | {"debug_assertions", ("target_abi", "")}
            ),
        )
        with mock.patch.object(
            notices.subprocess, "check_output", return_value="target os\n"
        ):
            with self.assertRaisesRegex(
                ValueError, "rustc printed the cfg option 'target os'"
            ):
                notices.target_platform(LINUX)


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
            CLI=self.root / "rust/crates/yamaa-cli",
            TARGETS=(LINUX, WINDOWS),
        )
        patches.start()
        self.addCleanup(patches.stop)
        (self.root / "LICENSE").write_bytes(PROJECT_LICENSE)
        for directory in (notices.PYTHON, notices.R_INST, notices.CLI):
            directory.mkdir(parents=True)
        (notices.CLI / notices.MUSL_NOTICE).write_text(MUSL_NOTICE)
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

    def built(self):
        """The graph's metadata and each host's closure, which check() and
        write() take."""
        return self.graph.metadata(), self.graph.closures()

    def test_the_notices_list_the_crates_linked_on_any_target(self):
        metadata, closures = self.graph.metadata(), self.graph.closures()
        # Neither binding links a build script's crate, a procedural macro or
        # the crates it uses, a dev-dependency, or r-efi, which only UEFI
        # links; windows-sys is linked on Windows alone.
        for host, own in (
            ("python", ["pyo3", "windows-sys"]),
            ("r", ["extendr-api"]),
            ("cli", []),
        ):
            with self.subTest(host=host):
                self.assertEqual(
                    [
                        package["name"]
                        for package in notices.third_party(metadata, closures[host])
                    ],
                    sorted(["arrow", "flatbuffers", "yamaa-saphyr-parser"] + own),
                )
        # The members that a binding links are in its closure, not its notices.
        self.assertEqual(
            self.names(closures["python"]),
            {"yamaa-python", "yamaa-core", "arrow", "flatbuffers", "pyo3"}
            | {"yamaa-saphyr-parser", "windows-sys"},
        )
        with mock.patch.object(
            notices.subprocess, "check_output", side_effect=self.graph.printed
        ):
            linux = notices.build_closure("yamaa-python", (), LINUX)
        self.assertEqual(
            self.names(closures["python"]) - {name for name, _ in linux},
            {"windows-sys"},
        )

    def test_each_closure_is_what_cargo_tree_prints_for_the_build(self):
        printed = (
            "yamaa-python v0.4.0 (/checkout/rust/crates/yamaa-python)\n"
            "pyo3 v0.27.2\n"
            "yamaa-saphyr-parser v0.2.1 (C:\\a (x86)\\rust\\vendor\\saphyr-parser)\n"
            "zstd-sys v2.1.2+zstd.1.5.7\n"
            "pyo3 v0.27.2 (*)\n"
        )
        with mock.patch.object(
            notices.subprocess, "check_output", return_value=printed
        ) as run:
            found = notices.build_closure("yamaa-python", ("extension-module",), LINUX)
        self.assertEqual(
            found,
            {
                ("yamaa-python", "0.4.0"),
                ("pyo3", "0.27.2"),
                ("yamaa-saphyr-parser", "0.2.1"),
                ("zstd-sys", "2.1.2+zstd.1.5.7"),
            },
        )
        # The package's own build, its features, one target and the normal
        # edges, which leave out dev-dependencies, build scripts' dependencies
        # and procedural macros.
        run.assert_called_once_with(
            [
                "cargo",
                "tree",
                "--locked",
                "--package",
                "yamaa-python",
                "--features",
                "extension-module",
                "--target",
                LINUX,
                "--edges",
                "normal,no-proc-macro",
                "--prefix",
                "none",
                "--format",
                "{p}",
            ],
            cwd=notices.WORKSPACE,
            encoding="utf-8",
        )
        # A build without features beyond the defaults names none.
        with mock.patch.object(
            notices.subprocess, "check_output", return_value="yamaa-r v0.4.0 (/r)\n"
        ) as run:
            self.assertEqual(
                notices.build_closure("yamaa-r", (), WINDOWS), {("yamaa-r", "0.4.0")}
            )
        self.assertNotIn("--features", run.call_args.args[0])
        # A line with a tree prefix is not a package.
        with mock.patch.object(
            notices.subprocess, "check_output", return_value="|-- pyo3 v0.27.2\n"
        ):
            with self.assertRaisesRegex(
                ValueError,
                re.escape(
                    f"cargo tree printed '|-- pyo3 v0.27.2' for yamaa-r on {LINUX}, "
                    "not a package"
                ),
            ):
                notices.build_closure("yamaa-r", (), LINUX)

    def test_each_host_links_the_closure_of_its_release_build(self):
        builds = []

        def printed(command, **options):
            builds.append(command)
            return self.graph.printed(command, **options)

        with mock.patch.object(notices.subprocess, "check_output", side_effect=printed):
            closures = notices.linked_packages(self.graph.metadata())
        self.assertEqual(set(closures), {"python", "r", "cli"})
        # Each host's crate, with its build's features, on each target.
        self.assertEqual(
            [
                (
                    command[command.index("--package") + 1],
                    command[command.index("--target") + 1],
                    "--features" in command
                    and command[command.index("--features") + 1],
                )
                for command in builds
            ],
            [
                (crate, target, features)
                for crate, features in (
                    ("yamaa-python", "extension-module"),
                    ("yamaa-r", False),
                    ("yamaa-cli", False),
                )
                for target in (LINUX, WINDOWS)
            ],
        )
        self.assertEqual(
            self.names(closures["r"]),
            {"yamaa-r", "yamaa-core", "arrow", "flatbuffers", "extendr-api"}
            | {"yamaa-saphyr-parser"},
        )
        # A printed package that the locked graph does not hold once, and a
        # closure without the crate itself, stop generation.
        python = self.graph.printed(["--package", "yamaa-python", "--target", LINUX])
        for output, message in (
            (
                python + "rogue v1.0.0\n",
                f"yamaa-python links rogue 1.0.0 on {LINUX}, which the locked graph "
                "holds 0 times, not once",
            ),
            ("arrow v57.3.0\n", f"cargo tree printed no yamaa-python for {LINUX}"),
        ):
            with (
                self.subTest(message=message),
                mock.patch.object(
                    notices.subprocess, "check_output", return_value=output
                ),
                self.assertRaisesRegex(ValueError, re.escape(message)),
            ):
                notices.linked_packages(self.graph.metadata())
        metadata = self.graph.metadata()
        metadata["packages"].append(dict(self.graph.packages["arrow@57.3.0"], id="x"))
        with self.assertRaisesRegex(ValueError, "arrow 57.3.0 on .* holds 2 times"):
            with mock.patch.object(
                notices.subprocess, "check_output", side_effect=self.graph.printed
            ):
                notices.linked_packages(metadata)

    def test_texts_follow_the_relied_license_and_fall_back_to_spdx(self):
        metadata, closures = self.graph.metadata(), self.graph.closures()
        selected = {
            notice.name: notice
            for notice in notices.notices_for(metadata, closures["python"])
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
            notices.license_expression(
                metadata, closures["python"], selected.values(), LIBRARY
            ),
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
        metadata, closures = self.graph.metadata(), self.graph.closures()
        (efi,) = [
            notice
            for notice in notices.notices_for(metadata, closures["r"])
            if notice.name == "r-efi"
        ]
        self.assertEqual(efi.relied, ("MIT",))
        with self.assertRaisesRegex(
            ValueError,
            r"the r notices: names a GNU license \(GNU Lesser General Public License\)",
        ):
            notices.render([efi], LIBRARY, "r")
        with self.assertRaisesRegex(ValueError, "names a GNU license"):
            notices.write(metadata, closures, LIBRARY)

    def test_zstd_sys_carries_the_bundled_library_text_it_is_relied_on_under(self):
        # zstd-sys compiles the bundled zstd library, offered under BSD-3-Clause
        # or GPL-2.0-only: the BSD text below the crate root must be rendered,
        # and the GPL text it is not relied on under must not.
        graph = Graph(self.root / "zstd")
        python = graph.crate("yamaa-python", "0.4.0", "MIT", member=True)
        graph.crate("yamaa-r", "0.4.0", "MIT", member=True)
        graph.crate("yamaa-cli", "0.4.0", "MIT", member=True)
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
        (notice,) = notices.notices_for(graph.metadata(), graph.closures()["python"])
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
        graph.crate("yamaa-cli", "0.4.0", "MIT", member=True)
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
        linked = graph.closures()["python"]
        selected = {
            notice.name: notice
            for notice in notices.notices_for(graph.metadata(), linked)
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
            notices.notices_for(graph.metadata(), linked)
        graph.packages[borrowing]["repository"] = repository
        directory = Path(graph.packages[borrowing]["manifest_path"]).parent
        (directory / "LICENSE").write_text(BSD)
        with self.assertRaisesRegex(ValueError, "drop it from SHARED_TEXTS"):
            notices.notices_for(graph.metadata(), linked)

    def test_the_standard_library_section_lists_the_crates_it_links(self):
        notice = notices.notices_for(
            self.graph.metadata(), self.graph.closures()["python"]
        )
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
        # The crates that std links, from the toolchain's rust-src component,
        # each with the one license relied on.
        self.assertIn(
            "From the rust-src component of Rust 1.99.1, lib/rustlib/src/rust/library, "
            "the\ncrates that the standard library links, and those of its in-tree "
            "crates that\ndeclare a license of their own:\n\n"
            "Crate: windows-sys 0.61.2\n    License: MIT\n    Texts: [",
            standard,
        )
        self.assertNotIn("r-efi", rendered)
        self.assertIn(
            "Shipped by: Rust 1.99.1 (share/doc/rust/licenses/MIT.txt)\n", texts
        )
        self.assertIn("from\nrust/Cargo.lock and the Rust 1.99.1 toolchain", rendered)
        # Only the command line's notices name musl.
        self.assertNotIn("musl", rendered)
        # A linked crate whose package ships no text for its license takes the
        # canonical text.
        bare = notices.Notice("vex-sdk", "0.27.1", None, ("MIT",), (), ("MIT",))
        rendered = notices.render(
            notice, dataclasses.replace(LIBRARY, crates=(bare,)), "python"
        )
        self.assertIn("Crate: vex-sdk 0.27.1\n    License: MIT\n    Texts: [", rendered)
        self.assertIn("canonical MIT text; the crate package ships none", rendered)
        # An in-tree crate names its directory and every license relied on.
        builtins = notices.Notice(
            "compiler_builtins",
            "0.1.160",
            "in-tree, compiler-builtins/compiler-builtins",
            ("MIT", "Apache-2.0 WITH LLVM-exception"),
            (("compiler-builtins/LICENSE.txt", BUILTINS),),
            (),
        )
        rendered = notices.render(
            notice, dataclasses.replace(LIBRARY, crates=(builtins,)), "python"
        )
        self.assertIn(
            "Crate: compiler_builtins 0.1.160\n"
            "    Source: in-tree, compiler-builtins/compiler-builtins\n"
            "    License: MIT AND Apache-2.0 WITH LLVM-exception\n"
            "    Texts: [",
            rendered,
        )

    def test_the_expression_covers_the_crates_that_the_standard_library_links(self):
        # The wheels link the standard library, whose compiler_builtins is
        # relied on under MIT AND Apache-2.0 WITH LLVM-exception, so the
        # expression names Apache-2.0 WITH LLVM-exception too (#3030 D185).
        metadata, closures = self.built()
        builtins = notices.Notice(
            "compiler_builtins",
            "0.1.160",
            "in-tree, compiler-builtins/compiler-builtins",
            ("MIT", "Apache-2.0 WITH LLVM-exception"),
            (("compiler-builtins/LICENSE.txt", BUILTINS),),
            (),
        )
        library = dataclasses.replace(LIBRARY, crates=(*LIBRARY.crates, builtins))
        expression = (
            "MIT AND Apache-2.0 AND Unicode-3.0 AND Apache-2.0 WITH LLVM-exception"
        )
        selected = notices.notices_for(metadata, closures["python"])
        self.assertEqual(
            notices.license_expression(metadata, closures["python"], selected, library),
            expression,
        )
        # The check refuses a project license that leaves it out.
        notices.write(metadata, closures, library)
        self.assertEqual(
            notices.check(metadata, closures, library)[1],
            [
                f"python/pyproject.toml: license must be {expression!r} to cover "
                "the linked crates, not 'MIT AND Apache-2.0 AND Unicode-3.0'"
            ],
        )
        self.project(license=expression)
        self.assertEqual(notices.check(metadata, closures, library)[1], [])

    def test_the_command_line_notices_name_musl_for_the_musl_targets(self):
        metadata, closures = self.built()
        cli = notices.render(
            notices.notices_for(metadata, closures["cli"]), LIBRARY, "cli"
        )
        section = cli.partition("\nmusl C library\n--------------\n")[2]
        section = section.partition("\nTexts\n-----\n")[0]
        self.assertIn(
            "The yamaa executables for x86_64-unknown-linux-musl and "
            "aarch64-unknown-linux-musl link musl, the C library that Rust 1.99.1 "
            "bundles for those targets, statically. Their archives carry its "
            "notice as LICENSE-MUSL.txt",
            " ".join(section.split()),
        )
        self.assertIn(
            f"Library: musl {notices.MUSL[1]}\n    License: MIT\n"
            "    Text: LICENSE-MUSL.txt\n",
            section,
        )
        self.assertIn(
            "The archives for the musl targets also carry LICENSE-MUSL.txt, the "
            "notice of musl.",
            " ".join(cli.split()),
        )
        for host in ("python", "r"):
            with self.subTest(host=host):
                selected = notices.notices_for(metadata, closures[host])
                self.assertNotIn("musl", notices.render(selected, LIBRARY, host))
        # Another toolchain's musl needs reviewing first.
        newer = dataclasses.replace(LIBRARY, release="1.100.0")
        with self.assertRaisesRegex(
            ValueError,
            "the musl notice was reviewed for Rust 1.99.1, not 1.100.0: set MUSL to "
            "the musl release that Rust 1.100.0 bundles for x86_64-unknown-linux-musl "
            "and aarch64-unknown-linux-musl, and LICENSE-MUSL.txt to its COPYRIGHT",
        ):
            notices.render(notices.notices_for(metadata, closures["cli"]), newer, "cli")

    def test_the_command_line_keeps_the_notice_of_musl(self):
        built = self.built()
        notices.write(*built, LIBRARY)
        self.assertEqual(notices.check(*built, LIBRARY)[1], [])
        path = notices.CLI / notices.MUSL_NOTICE
        relative = "rust/crates/yamaa-cli/LICENSE-MUSL.txt"
        header = f"musl {notices.MUSL[1]}\n{notices.MUSL_HOME}\n\n"
        for text, error in (
            (
                MUSL_NOTICE.replace(notices.MUSL[1], "1.2.3", 1),
                f"{relative}: must start with the header {header!r}",
            ),
            (
                MUSL_NOTICE.replace("hereby granted, free of charge", "granted"),
                f"{relative}: lacks musl's MIT license",
            ),
            (
                MUSL_NOTICE + "Some files are under the GNU General Public License.\n",
                f"{relative}: names a GNU license (GNU General Public License)",
            ),
        ):
            path.write_text(text)
            with self.subTest(error=error):
                self.assertEqual(notices.check(*built, LIBRARY)[1], [error])
        path.unlink()
        self.assertEqual(
            notices.check(*built, LIBRARY)[1],
            [f"{relative}: missing; the archives for the musl targets carry it"],
        )

    def test_check_fails_when_the_closure_gains_or_loses_a_crate(self):
        built = self.built()
        notices.write(*built, LIBRARY)
        self.assertEqual(notices.check(*built, LIBRARY)[1], [])
        self.assertEqual((notices.PYTHON / "LICENSE.txt").read_bytes(), PROJECT_LICENSE)
        # The command-line archives carry the Python packages' license copy.
        self.assertFalse((notices.CLI / "LICENSE.txt").exists())
        hosts = (
            ("python", "yamaa-python"),
            ("R/yamaa/inst", "yamaa-r"),
            ("rust/crates/yamaa-cli", "yamaa-cli"),
        )
        # A toolchain whose notice changed needs the notices regenerated.
        changed = dataclasses.replace(
            LIBRARY, statement=("Copyrights are retained by their contributors.",)
        )
        self.assertEqual(
            notices.check(*built, changed)[1],
            [
                f"{relative}/LICENSE-THIRD-PARTY.txt: differs from the notices "
                f"generated for {crate} from rust/Cargo.lock and the Rust 1.99.1 "
                "toolchain"
                for relative, crate in hosts
            ],
        )
        gained = self.graph.crate("snap", "1.1.2", "BSD-3-Clause", {"COPYING": BSD})
        self.graph.depend("yamaa-core@0.4.0", gained)
        errors = notices.check(*self.built(), LIBRARY)[1]
        for relative, crate in hosts:
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
        errors = notices.check(*self.built(), LIBRARY)[1]
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
        built = self.built()
        selected = notices.write(*built, LIBRARY)
        python = (notices.PYTHON / "LICENSE-THIRD-PARTY.txt").read_text()
        r = (notices.R_INST / "LICENSE-THIRD-PARTY.txt").read_text()
        cli = (notices.CLI / "LICENSE-THIRD-PARTY.txt").read_text()
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
        self.assertEqual(notices.committed_entries(cli), shared)
        self.assertEqual(
            {host: len(found) for host, found in selected.items()},
            {"python": 5, "r": 4, "cli": 3},
        )
        for text, packages, linked, crate, features in (
            (
                python,
                "The yamaa Python wheels link",
                " into their native extension.",
                "yamaa-python",
                "its default features and the feature extension-module",
            ),
            (
                r,
                "The installed yamaa R package links",
                " into its native extension.",
                "yamaa-r",
                "its default features",
            ),
            (
                cli,
                "The yamaa command-line binaries link",
                # Only the executables for the musl targets link musl.
                ", and on the musl targets the musl C library, into the yamaa "
                "executable.",
                "yamaa-cli",
                "its default features",
            ),
        ):
            flat = " ".join(text.split())
            self.assertIn(f"{packages} third-party Rust crates", flat)
            self.assertIn(f"the Rust standard library{linked}", flat)
            # The list follows the features that the host's release build
            # enables.
            self.assertIn(
                f"crates that the shipped crate {crate} links, as locked in "
                f"rust/Cargo.lock and built with {features}: its normal dependency "
                f"closure on each of the targets {LINUX}, {WINDOWS}, without "
                "procedural macros. Crates that only build scripts or procedural "
                "macros use, and crates that only other features or other "
                "platforms select, are not linked",
                flat,
            )
            # Every host links the standard library and lists its linked crates.
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
            notices.check(*built, LIBRARY)[1],
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
        built = self.built()
        notices.write(*built, LIBRARY)
        (notices.R_INST / "LICENSE-YAML.txt").write_text("changed\n")
        (notices.R_INST / "LICENSE-WINDOWS.txt").write_text("stale\n")
        (notices.PYTHON / "LICENSE.txt").write_text("edited\n")
        # The command line keeps no copy of a notice that the hosts share.
        (notices.CLI / "LICENSE-YAML.txt").write_text("yaml\n")
        self.project(
            license="MIT AND Apache-2.0 AND Unicode-3.0",
            files=("LICENSE.txt", "LICENSE-YAML.txt"),
        )
        errors = notices.check(*built, LIBRARY)[1]
        for expected in (
            "python/pyproject.toml: license-files must list LICENSE-THIRD-PARTY.txt",
            "R/yamaa/inst/LICENSE-YAML.txt: must be identical to python/LICENSE-YAML.txt",
            "R/yamaa/inst/LICENSE.txt: must be identical to python/LICENSE.txt",
            "R/yamaa/inst/LICENSE-WINDOWS.txt: not a package notice in license-files",
            "python/LICENSE-THIRD-PARTY.txt: not a package notice in license-files",
            "python/LICENSE.txt: must be a copy of LICENSE",
            "rust/crates/yamaa-cli/LICENSE-YAML.txt: the command line keeps only "
            "its own LICENSE-THIRD-PARTY.txt, COMPONENTS.tsv and LICENSE-MUSL.txt",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, errors)

    def test_only_the_notices_and_the_component_inventory_differ_between_hosts(self):
        # Each host's packages carry the notices and the inventory of their own
        # crates; tools.packaging.generate_component_inventory checks the
        # inventories.
        built = self.built()
        notices.write(*built, LIBRARY)
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
            notices.check(*built, LIBRARY)[1],
            ["python/COMPONENTS.tsv: listed in license-files but missing"],
        )
        (notices.PYTHON / notices.INVENTORY).write_text("name\npyo3\n")
        (notices.R_INST / notices.INVENTORY).write_text("name\nextendr-api\n")
        self.assertEqual(notices.check(*built, LIBRARY)[1], [])
        (notices.R_INST / "LICENSE-YAML.txt").write_text("changed\n")
        self.assertEqual(
            notices.check(*built, LIBRARY)[1],
            [
                "R/yamaa/inst/LICENSE-YAML.txt: must be identical to "
                "python/LICENSE-YAML.txt"
            ],
        )


def cargo_build(command):
    """The package, the features and the other feature options of a cargo
    build command."""
    words = command.split()
    package, features, options = None, [], []
    for index, word in enumerate(words):
        following = words[index + 1] if index + 1 < len(words) else None
        if word in ("-p", "--package"):
            package = following
        elif word.startswith("--package="):
            package = word.removeprefix("--package=")
        elif word in ("-F", "--features"):
            features += following.split(",")
        elif word.startswith("--features="):
            features += word.removeprefix("--features=").split(",")
        elif word in ("--no-default-features", "--all-features"):
            options.append(word)
    return package, tuple(features), options


class ReleaseBuilds(unittest.TestCase):
    """Each host's notices follow the features of its release build."""

    def test_the_build_features_are_those_of_each_release_build(self):
        # The wheels: the features that maturin builds the binding with.
        project = tomllib.loads(
            (notices.PYTHON / "pyproject.toml").read_text(encoding="utf-8")
        )
        maturin = project["tool"]["maturin"]
        self.assertTrue(
            maturin["manifest-path"].endswith(
                f"/crates/{engine_bundle.BINDINGS['python']}/Cargo.toml"
            )
        )
        self.assertEqual(
            tuple(maturin.get("features", ())), notices.BUILD_FEATURES["python"]
        )
        for option in ("no-default-features", "all-features"):
            with self.subTest(option=option):
                self.assertFalse(maturin.get(option, False), option)
        # The R package: the one cargo build of each Makevars template.
        for name in ("Makevars.in", "Makevars.win.in"):
            text = (notices.ROOT / "R/yamaa/src" / name).read_text(encoding="utf-8")
            (command,) = [
                line
                for line in text.replace("\\\n", " ").splitlines()
                if '"$(CARGO)" build ' in line
            ]
            with self.subTest(makevars=name):
                self.assertEqual(
                    cargo_build(command),
                    (engine_bundle.BINDINGS["r"], notices.BUILD_FEATURES["r"], []),
                )
        # The command-line binaries: the native-cli lane's cargo build.
        workflow = yaml.safe_load(
            (notices.ROOT / ".github/workflows/native.yml").read_text(encoding="utf-8")
        )
        steps = {step.get("name"): step for step in workflow["jobs"]["cli"]["steps"]}
        (command,) = [
            line
            for line in steps["Build the command-line binary"]["run"].splitlines()
            if "cargo build" in line
        ]
        self.assertEqual(
            cargo_build(command),
            (engine_bundle.BINDINGS["cli"], notices.BUILD_FEATURES["cli"], []),
        )

    def test_a_build_command_names_its_package_and_feature_options(self):
        self.assertEqual(
            cargo_build(
                "cargo build --locked -p yamaa-r --features a,b -F c "
                "--features=d --no-default-features"
            ),
            ("yamaa-r", ("a", "b", "c", "d"), ["--no-default-features"]),
        )
        self.assertEqual(
            cargo_build("cargo build --package=yamaa-cli --all-features"),
            ("yamaa-cli", (), ["--all-features"]),
        )


if __name__ == "__main__":
    unittest.main()
