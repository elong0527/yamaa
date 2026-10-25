"""Generate the license notices for the third-party code in the native packages.

The yamaa wheels, the installed R package and the command-line binaries
statically link Rust crates from the locked graph. Each host's
LICENSE-THIRD-PARTY.txt lists the third-party crates that the host's shipped
crate links: its normal dependency closure on each of the supported TARGETS,
with the features that the host's release build enables (BUILD_FEATURES),
without procedural macros, which run inside the compiler. Crates that only
build scripts or procedural macros use, and crates that only other features
or other platforms select, are not linked and are not listed; the source
packages carry their license files with their vendored sources. python/ holds
the copy of the Python packages, which covers yamaa-python, R/yamaa/inst/ the
copy of the R package, which covers yamaa-r, and rust/crates/yamaa-cli/ the
copy of the command-line archives, which covers yamaa-cli. Each entry names
the one license that yamaa relies on, with the license, copyright and NOTICE
texts that the crate ships for it. Each closure comes from `cargo tree` over
rust/Cargo.lock (build_closure), the crates' metadata from `cargo metadata
--locked`, and the texts from the crate packages Cargo unpacks.

The native code also links the Rust standard library of the toolchain that
rust/rust-toolchain.toml pins. Its licensing statement and the licenses of its
own files come from the notice that the Rust project ships with that
toolchain, share/doc/rust/COPYRIGHT-library.html, and the license texts beside
it. That notice leaves out some of the third-party crates that the standard
library links, so those crates come from the toolchain's rust-src component
instead (#3369): the crates that std links on each of the TARGETS, as
library/std/Cargo.toml declares them for the features that LIBRARY_FEATURES
names and library/Cargo.lock locks them, each with the texts that its package
in library/vendor ships, chosen as for a locked crate. The in-tree crates
that std links and whose manifests declare a license that the notice does
not give the standard library's files, such as compiler_builtins, are listed
with them, with the license files that LIBRARY_IN_TREE names (#3494). The
repository checks
add rust-src to the pinned toolchain (tools/ci/toolchain.py); where it cannot
be added, the environment variable that LIBRARY_SOURCES_VARIABLE names gives
the library directory of an unpacked copy of the same component. The
command-line binaries for the musl targets also link musl, whose notice the
command line keeps beside its crate as MUSL_NOTICE for their archives alone.
No license text is looked up anywhere else.

No notice names a GNU license: generation stops when a reproduced text would
(gnu_licenses), apart from the clause of the LLVM exception, an accepted
license, that names GPLv2.

By default this checks the committed copies against the locked graph and the
pinned toolchain, the package copies of the project license and notices
against their sources, and the Python License-Expression against the licenses
that the crates linked into the Python binding, and those that the standard
library links, are relied on under (#3030 D185). `--write` regenerates the
notices and the license copies.
"""

import argparse
import json
import os
import re
import subprocess
import textwrap
import tomllib
from dataclasses import dataclass
from html.parser import HTMLParser
from pathlib import Path

from tools.ci import native, toolchain
from tools.packaging import check_minimum_rust
from tools.paths import ROOT, WORKSPACE
from tools.staging import engine_bundle

NOTICES = "LICENSE-THIRD-PARTY.txt"
PROJECT_LICENSE = "LICENSE.txt"
# The component inventory: tools/packaging/generate_component_inventory.py
# writes each host's copy from its trimmed lock and checks them.
INVENTORY = "COMPONENTS.tsv"
# The package notices whose hosts' copies differ, because each lists the
# components of its own host; the hosts share the other package notices.
HOST_NOTICES = (NOTICES, INVENTORY)
# musl, the C library that the Rust toolchain bundles for its *-linux-musl
# targets, which the command-line binaries for MUSL_TARGETS link statically
# (#3369). Only their archives carry its notice: a two-line header naming the
# musl release and its home, then musl's COPYRIGHT file verbatim. MUSL pairs
# the toolchain release whose bundled musl was reviewed with that musl
# release, which src/ci/docker/scripts/musl.sh of the Rust repository names
# at the toolchain's tag; another pinned toolchain stops the check until the
# pair and the notice are reviewed.
MUSL_NOTICE = "LICENSE-MUSL.txt"
MUSL = ("1.99.1", "1.2.5")
MUSL_HOME = "https://musl.libc.org/"
MUSL_TARGETS = tuple(
    target for target in native.CLI_TARGETS.values() if target.endswith("-linux-musl")
)
# The Python project lists the package notices; the R package installs its
# copies from inst/. The command line keeps only its CLI_NOTICES beside its
# crate: its archives carry the Python packages' copies of the other notices
# (tools/packaging/cli_archive.py).
CLI_NOTICES = (*HOST_NOTICES, MUSL_NOTICE)
PYTHON = ROOT / "python"
R_INST = ROOT / "R/yamaa/inst"
CLI = ROOT / "rust/crates/yamaa-cli"
# How each host's notices name the packages that link the crates and what the
# crates are linked into.
LINKED_BY = {
    "python": (
        "The yamaa Python wheels link",
        "their native extension",
    ),
    "r": (
        "The installed yamaa R package links",
        "its native extension",
    ),
    "cli": (
        "The yamaa command-line binaries link",
        "the yamaa executable",
    ),
}
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
# The targets whose linked crates the notices list: those that the wheels, the
# command-line binaries and the R package are built for.
TARGETS = check_minimum_rust.TARGETS
# The features that each host's release build enables besides its binding's
# default features, which decide the crates that the binding links (#3470).
# maturin builds the wheels with the features that [tool.maturin] in
# python/pyproject.toml names; the one cargo build of R/yamaa/src/Makevars.in
# and Makevars.win.in builds the R package, and the native-cli lane of
# .github/workflows/native.yml the command-line binaries, with no feature
# options. tools/tests/test_third_party_notices.py holds this table to those
# builds.
BUILD_FEATURES = {
    "python": ("extension-module",),
    "r": (),
    "cli": (),
}
# A package as `cargo tree --format {p}` prints it: its name, its version after
# a v, and for a package that is not from crates.io its source in parentheses.
TREE_PACKAGE = re.compile(r"(\S+) v(\S+)(?: \(.+\))?")
SPDX_TEXTS = Path(__file__).with_name("spdx")
SPDX_RELEASE = "3.29.0"
WIDTH = 78

# Licenses a shipped crate may be relied on under, in preference order. Where a
# crate offers a choice, the first offered license whose text the crate ships
# is relied on. A crate usable only under another license stops generation
# until its license has been reviewed and added here. Zlib is zlib-rs's only
# license: the adapters select that flate2 backend, as Parquet 55 did (#3878).
ACCEPTED = (
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "Unicode-3.0",
    "Apache-2.0 WITH LLVM-exception",
    "CC0-1.0",
    "Zlib",
)

# Phrases, compared case-insensitively with whitespace collapsed, that each
# accepted license's text contains. They confirm that a reproduced file holds
# the license it is reproduced for; they never select a file.
MARKERS = {
    "MIT": (
        "permission is hereby granted, free of charge, to any person obtaining a copy",
    ),
    "Apache-2.0": ("terms and conditions for use, reproduction, and distribution",),
    "BSD-3-Clause": (
        "redistribution and use in source and binary forms",
        "neither the name",
    ),
    "Unicode-3.0": ("unicode license v3",),
    "Apache-2.0 WITH LLVM-exception": (
        "terms and conditions for use, reproduction, and distribution",
        "llvm exceptions to the apache 2.0 license",
    ),
    "CC0-1.0": ("cc0 1.0 universal",),
    "Zlib": (
        "this software is provided 'as-is', without any express or implied warranty",
        "permission is granted to anyone to use this software for any purpose",
    ),
}

# A GNU license named in a text: its full name, or an abbreviation such as
# GPLv2, LGPL-2.1-or-later or AGPL. The abbreviations match in capitals only,
# so that an identifier such as `lgpl` is not one.
GNU_LICENSE = re.compile(
    r"(?i:\bGNU\s+(?:Lesser\s+|Library\s+|Affero\s+)?General\s+Public\s+Licen[cs]e)"
    r"|\b[AL]?GPL(?:v?[0-9](?:\.[0-9])?)?\b"
)
# The clause of the LLVM exception that names GPLv2, with its whitespace
# collapsed. It waives parts of Apache-2.0 for code combined with GPLv2 code
# and grants no GNU license; Apache-2.0 WITH LLVM-exception is in ACCEPTED.
LLVM_EXCEPTION_CLAUSE = re.compile(
    r"In addition, if you combine or link compiled forms of this Software with "
    r'software that is licensed under the GPLv2 \("Combined Software"\) and if '
    r"a court of competent jurisdiction determines that the patent provision "
    r"\(Section 3\), the indemnity provision \(Section 9\) or other Section of "
    r"the License conflicts with the conditions of the GPLv2, you may "
    r"retroactively and prospectively choose to deem waived or otherwise "
    r"exclude such Section\(s\) of the License, but only in their entirety and "
    r"only with respect to the Combined Software\."
)

# Root files named for one license of a crate's choice, by lowercase name. Each
# is reproduced only when the crate is relied on under that license.
NAMED_FILES = {
    "license-0bsd": "0BSD",
    "license-apache": "Apache-2.0",
    "license-apache-2.0": "Apache-2.0",
    "license-apache-2.0_with_llvm-exception": "Apache-2.0 WITH LLVM-exception",
    "license-apache.md": "Apache-2.0",
    "license-boost": "BSL-1.0",
    "license-bsd": "BSD-2-Clause",
    "license-mit": "MIT",
    "license-mit.md": "MIT",
    "license-unicode": "Unicode-3.0",
    "license-zlib.md": "Zlib",
    "license.bsd-3-clause": "BSD-3-Clause",
    "license.mit": "MIT",
    "unlicense": "Unlicense",
}
# Root files reproduced whatever license is relied on: they hold copyright and
# attribution notices, statements of the licensing terms, or the crate's only
# license text.
GENERAL_FILES = {
    "authors",
    "copying",
    "copyright",
    "licence",
    "license",
    "license.md",
    "license.txt",
    "notice",
    "notice.md",
    "notice.txt",
}
# Any other license-like root file stops generation until it is classified.
LICENSE_LIKE = re.compile(
    r"(?i)^(?:authors|copying|copyright|licen[cs]e|notice|patents|unlicense)"
)
# Notices that ship below a crate's root, with the license each carries. A
# listed file must exist, so a crate that moves one stops generation.
EXTRA_FILES = {
    # The vendored parser keeps both authors' license texts in LICENSES/.
    "yamaa-saphyr-parser": {
        "LICENSES/ChenYuheng-Apache": "Apache-2.0",
        "LICENSES/ChenYuheng-MIT": "MIT",
        "LICENSES/Ethiraric-Apache": "Apache-2.0",
        "LICENSES/Ethiraric-MIT": "MIT",
    },
    # zstd-sys compiles the bundled zstd library, which is offered under
    # BSD-3-Clause or GPL-2.0-only; the crate is relied on under BSD-3-Clause.
    "zstd-sys": {"zstd/COPYING": "GPL-2.0-only", "zstd/LICENSE": "BSD-3-Clause"},
}
# Crates whose package ships no notice, each mapped to a crate published from
# the same repository whose package ships that repository's license. The mapped
# crate's texts are reproduced for both. Generation stops if the two crates
# name different repositories or the first starts shipping a notice of its own.
SHARED_TEXTS = {
    # dropbox/rust-alloc-no-stdlib publishes both crates; only alloc-no-stdlib
    # packages the repository's BSD-3-Clause LICENSE.
    "alloc-stdlib": "alloc-no-stdlib",
}

# The Rust project's notice for the standard library of a toolchain, and the
# license texts it names, below the toolchain's sysroot.
LIBRARY_DOCUMENTS = "share/doc/rust"
LIBRARY_NOTICE = "COPYRIGHT-library.html"
LIBRARY_LICENSES = "licenses"
# The notice's sections, by HTML id, in order; the table of contents has none.
# Another shape stops generation. Only the statement and the in-tree entries
# are read: the out-of-tree section misses crates that std links, such as
# addr2line and object, which the crates of LIBRARY_SOURCES stand for.
LIBRARY_SECTIONS = (
    None,
    "short-version",
    "longer-version",
    "in-tree-files",
    "out-of-tree-dependencies",
)
# The library directory of a toolchain's rust-src component, below the
# sysroot, and the environment variable that names an unpacked copy of it
# instead, for a toolchain that rustup cannot add the component to. The
# directory holds the standard library's workspace: library/Cargo.lock, the
# manifests of std and the other in-tree crates, and in vendor/ the package of
# each out-of-tree crate, as `cargo vendor --versioned-dirs` writes it.
LIBRARY_SOURCES = "lib/rustlib/src/rust/library"
LIBRARY_SOURCES_VARIABLE = "YAMAA_RUST_SRC_LIBRARY"
# The features of std that the Rust project builds the distributed standard
# library with: panic-unwind, which library/sysroot/Cargo.toml enables by
# default, and backtrace, which the Rust project's build system adds by
# default (its rust.backtrace setting) and which links the crates that
# symbolize backtraces.
LIBRARY_FEATURES = ("backtrace", "panic-unwind")
# The license that the notice gives the standard library's own files, which
# an in-tree crate's manifest declares in either order, if it declares any.
LIBRARY_STATEMENT = frozenset({("LICENSE", "MIT"), ("LICENSE", "Apache-2.0")})
# The in-tree crates that std links whose manifests declare another license,
# which the notice does not give them (#3494), each with the license files
# that its sources carry, relative to the library directory. Each is listed
# with the crates of LIBRARY_SOURCES and chosen like a crate. Another such
# crate stops generation until it is reviewed here.
LIBRARY_IN_TREE = {
    # A port of LLVM's compiler-rt, MIT AND Apache-2.0 WITH LLVM-exception,
    # which compiles in the code of libm, MIT, a port of musl's libm.
    "compiler_builtins": (
        "compiler-builtins/LICENSE.txt",
        "compiler-builtins/libm/LICENSE.txt",
    ),
}
# A cfg option that `rustc --print cfg` prints: a name, or a name and a value.
CFG_OPTION = re.compile(r'([A-Za-z_][A-Za-z0-9_]*)(?:="([^"\\]*)")?')
# The tokens of a cfg() expression that selects a manifest's target table.
CFG_TOKEN = re.compile(r'\s*(?:([A-Za-z_][A-Za-z0-9_]*)|"([^"\\]*)"|([(),=]))')


class LicenseReview(ValueError):
    """A crate can only be used under a license that is not accepted."""


@dataclass(frozen=True)
class Notice:
    name: str
    version: str
    source: str | None
    relied: tuple[str, ...]
    texts: tuple[tuple[str, str], ...]
    fallbacks: tuple[str, ...]


@dataclass(frozen=True)
class LibraryFiles:
    """A file or directory of the standard library's own tree and its terms."""

    path: str
    license: str
    copyrights: tuple[str, ...]
    exceptions: tuple["LibraryFiles", ...]


@dataclass(frozen=True)
class StandardLibrary:
    """The standard library of a toolchain: its notice's statement, in-tree
    entries and license texts, and the notices of the crates that it links."""

    release: str
    statement: tuple[str, ...]
    files: tuple[LibraryFiles, ...]
    licenses: tuple[tuple[str, str], ...]
    crates: tuple[Notice, ...]


@dataclass(frozen=True)
class Platform:
    """A target and the cfg options that rustc sets for it: names, such as
    unix, and (name, value) pairs, such as ("target_os", "linux")."""

    target: str
    options: frozenset


@dataclass(frozen=True)
class LibraryUnit:
    """A package of the standard library's workspace: its lock entry, its
    directory and its manifest."""

    locked: dict
    directory: Path
    manifest: dict

    @property
    def label(self):
        return f"{self.locked['name']} {self.locked['version']}"


def locked_metadata():
    """Describe the packages of the locked graph and its workspace members:
    the graph resolved with every feature of every workspace member, for
    every platform."""
    command = ["cargo", "metadata", "--locked", "--all-features"]
    command += ["--format-version", "1"]
    return json.loads(subprocess.check_output(command, cwd=WORKSPACE))


def build_closure(crate, features, target):
    """Return the (name, version) of each package that a build of a shipped
    crate with its default features and `features` links for a target, the
    crate itself included.

    `cargo tree` prints the graph that Cargo's feature resolver gives that one
    build: the package's normal dependencies that its features and the target
    select, without dev-dependencies, and without build scripts' dependencies
    and procedural macros, which run while compiling. The graph of `cargo
    metadata` cannot stand in for it, whatever its feature options: it
    resolves the whole workspace, with every member's dev-dependencies and
    the features that any platform's dependencies ask for, and it keeps an
    optional dependency that only a weak `dependency?/feature` item names, so
    it holds crates that no build compiles (#3470).
    """
    command = ["cargo", "tree", "--locked", "--package", crate]
    if features:
        command += ["--features", ",".join(features)]
    command += ["--target", target, "--edges", "normal,no-proc-macro"]
    command += ["--prefix", "none", "--format", "{p}"]
    printed = subprocess.check_output(command, cwd=WORKSPACE, encoding="utf-8")
    found = set()
    for line in printed.splitlines():
        # A package printed before ends in (*).
        match = TREE_PACKAGE.fullmatch(line.removesuffix(" (*)"))
        if match is None:
            raise ValueError(
                f"cargo tree printed {line!r} for {crate} on {target}, not a package"
            )
        found.add((match[1], match[2]))
    return found


def linked_packages(metadata):
    """Return the IDs of the packages that each host's shipped crate links on
    any of TARGETS, built as the host's release build builds it, by host."""
    identities = {}
    for package in metadata["packages"]:
        key = (package["name"], package["version"])
        identities.setdefault(key, []).append(package["id"])
    linked = {}
    for host in copies():
        crate = engine_bundle.BINDINGS[host]
        linked[host] = set()
        for target in TARGETS:
            closure = build_closure(crate, BUILD_FEATURES[host], target)
            for name, version in sorted(closure):
                found = identities.get((name, version), [])
                if len(found) != 1:
                    raise ValueError(
                        f"{crate} links {name} {version} on {target}, which the "
                        f"locked graph holds {len(found)} times, not once"
                    )
                linked[host].add(found[0])
            if not any(name == crate for name, _ in closure):
                raise ValueError(f"cargo tree printed no {crate} for {target}")
    return linked


def parse_expression(text):
    """Parse an SPDX license expression; Cargo's legacy `/` separator means OR."""
    tokens = re.findall(r"[()/]|[^\s()/]+", text)
    operators = {"(", ")", "/", "AND", "OR", "WITH"}
    position = 0

    def peek():
        return tokens[position] if position < len(tokens) else None

    def take():
        nonlocal position
        token = peek()
        position += 1
        return token

    def identifier():
        token = take()
        if token is None or token in operators:
            raise ValueError(f"malformed license expression: {text!r}")
        return token

    def term():
        if peek() == "(":
            take()
            node = disjunction()
            if take() != ")":
                raise ValueError(f"malformed license expression: {text!r}")
            return node
        token = identifier()
        if peek() == "WITH":
            take()
            token = f"{token} WITH {identifier()}"
        return ("LICENSE", token)

    def conjunction():
        parts = [term()]
        while peek() == "AND":
            take()
            parts.append(term())
        return parts[0] if len(parts) == 1 else ("AND", tuple(parts))

    def disjunction():
        options = [conjunction()]
        while peek() in {"OR", "/"}:
            take()
            options.append(conjunction())
        return options[0] if len(options) == 1 else ("OR", tuple(options))

    node = disjunction()
    if peek() is not None:
        raise ValueError(f"malformed license expression: {text!r}")
    return node


def rank(license):
    return ACCEPTED.index(license)


def choose(node, shipped):
    """Return the licenses relied on for an expression node.

    Every part of a conjunction is relied on. A choice takes the acceptable
    option whose texts the crate ships, preferring earlier ACCEPTED licenses.
    """
    kind, value = node
    if kind == "LICENSE":
        if value not in ACCEPTED:
            raise LicenseReview(value)
        return frozenset({value})
    if kind == "AND":
        return frozenset().union(*(choose(part, shipped) for part in value))
    options = []
    for option in value:
        try:
            options.append(choose(option, shipped))
        except LicenseReview:
            continue
    if not options:
        raise LicenseReview("no acceptable license is offered")
    return min(
        options, key=lambda option: (not option <= shipped, sorted(map(rank, option)))
    )


def ordered(licenses):
    return tuple(sorted(licenses, key=rank))


def detect(text):
    """Name the accepted licenses whose text appears in a file."""
    flat = " ".join(text.lower().split())
    return frozenset(
        license
        for license, phrases in MARKERS.items()
        if all(phrase in flat for phrase in phrases)
    )


def gnu_licenses(text):
    """Return the GNU license names in a text, in order, apart from the LLVM
    exception's GPLv2 clause."""
    flat = LLVM_EXCEPTION_CLAUSE.sub("", " ".join(text.split()))
    return [match.group(0) for match in GNU_LICENSE.finditer(flat)]


def refuse_gnu(text, label):
    """Stop generation when a notice names a GNU license."""
    found = gnu_licenses(text)
    if found:
        raise ValueError(
            f"{label}: names a GNU license ({found[0]}); yamaa relies only on "
            "the licenses in ACCEPTED, so review the text that brought it in"
        )
    return text


def normalized(text):
    """Give a notice LF line endings and a final newline."""
    text = text.replace("\r\n", "\n")
    return text if text.endswith("\n") else text + "\n"


def read_text(path):
    """Read a notice verbatim, with LF line endings and a final newline."""
    try:
        text = path.read_bytes().decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"{path}: notices must be UTF-8 text") from error
    return normalized(text)


def version_key(version):
    parts = re.split(r"[.+-]", version)
    return [(0, int(part)) if part.isdigit() else (1, part) for part in parts]


def copies():
    """The checkout directory of each host's package notices, by host."""
    return {"python": PYTHON, "r": R_INST, "cli": CLI}


def third_party(metadata, linked):
    """Return the third-party packages among the IDs of linked packages, in a
    stable order."""
    packages = {package["id"]: package for package in metadata["packages"]}
    members = set(metadata["workspace_members"])
    selected = [packages[identity] for identity in linked if identity not in members]
    return sorted(
        selected,
        key=lambda package: (package["name"], version_key(package["version"])),
    )


def classify(name, label):
    """Return the license that a notice file carries by its name, or None for
    a general file, which is reproduced under any choice."""
    lowered = name.lower()
    if lowered in GENERAL_FILES:
        return None
    if lowered in NAMED_FILES:
        return NAMED_FILES[lowered]
    raise ValueError(f"{label}: classify the license file {name}")


def crate_files(package):
    """Return the crate root and its notice files with the license each carries."""
    root = Path(package["manifest_path"]).parent
    label = f"{package['name']} {package['version']}"
    files = [
        (path.name, classify(path.name, label))
        for path in sorted(root.iterdir(), key=lambda path: path.name.lower())
        if LICENSE_LIKE.match(path.name) and path.is_file()
    ]
    for relative, license in sorted(EXTRA_FILES.get(package["name"], {}).items()):
        if not (root / relative).is_file():
            raise ValueError(f"{label}: the listed notice {relative} is missing")
        files.append((relative, license))
    return root, files


def source_label(package):
    """Name a crate's source; crates.io is the default and stays unnamed."""
    if package["source"] == CRATES_IO:
        return None
    if package["source"] is None:
        directory = Path(package["manifest_path"]).parent.resolve()
        return directory.relative_to(WORKSPACE.parent.resolve()).as_posix()
    raise ValueError(
        f"{package['name']} {package['version']}: unsupported source "
        f"{package['source']}"
    )


def crate_notice(package, lender=None):
    """Select the texts reproduced for one crate.

    A lender is the SHARED_TEXTS crate whose texts stand for the crate's own.
    """
    label = f"{package['name']} {package['version']}"
    declared = package.get("license")
    if not declared:
        raise ValueError(f"{label}: declares no license expression; review it")
    root, files = crate_files(package)
    if lender is not None:
        if files:
            raise ValueError(
                f"{label}: ships its own notices; drop it from SHARED_TEXTS"
            )
        lent = f"{lender['name']} {lender['version']}"
        root, lent_files = crate_files(lender)
        files = [(f"{relative} of {lent}", license) for relative, license in lent_files]
        texts = {
            f"{relative} of {lent}": read_text(root / relative)
            for relative, _ in lent_files
        }
    else:
        texts = {relative: read_text(root / relative) for relative, _ in files}
    return Notice(
        package["name"],
        package["version"],
        source_label(package),
        *relied_texts(label, declared, files, texts),
    )


def relied_texts(label, declared, files, texts):
    """Choose the licenses relied on under a declared expression and the texts
    reproduced for them.

    `files` pairs each notice file's name with the license it carries, None
    for a general file, and `texts` maps the names to their texts. The general
    files and the files of the licenses relied on are reproduced; a license
    that none of them carries takes its canonical SPDX text. Returns the
    licenses, the reproduced (name, text) pairs and those licenses.
    """
    shipped = frozenset().union(*(detect(text) for text in texts.values()))
    try:
        relied = choose(parse_expression(declared), shipped)
    except LicenseReview as error:
        raise ValueError(
            f"{label}: review the license {declared!r} ({error})"
        ) from error
    reproduced = []
    for name, license in files:
        if license is None:
            reproduced.append((name, texts[name]))
        elif license in relied:
            if license not in detect(texts[name]):
                raise ValueError(f"{label}: {name} lacks the {license} text")
            reproduced.append((name, texts[name]))
    fallbacks = []
    for license in ordered(relied):
        if any(license in detect(text) for _, text in reproduced):
            continue
        if not (SPDX_TEXTS / f"{license}.txt").is_file():
            raise ValueError(f"{label}: ships no {license} text to reproduce")
        fallbacks.append(license)
    return ordered(relied), tuple(reproduced), tuple(fallbacks)


def canonical_text(license):
    return read_text(SPDX_TEXTS / f"{license}.txt")


class LibraryNotice(HTMLParser):
    """Read the statement and the in-tree entries of the Rust project's
    standard library notice.

    The notice is the HTML that the Rust project's generate-copyright tool
    renders: the licensing statement, nested in-tree entries, and one heading
    per out-of-tree crate, a section that is not read (LIBRARY_SECTIONS). A
    paragraph outside that shape stops generation instead of being skipped.
    """

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.sections, self.statement, self.files = [], [], []
        self.open = []  # in-tree entries being read, outermost first
        self.text = None

    @property
    def section(self):
        return self.sections[-1] if self.sections else None

    def handle_starttag(self, tag, attrs):
        if tag == "h2":
            self.sections.append(dict(attrs).get("id"))
        elif self.section == "out-of-tree-dependencies":
            return
        elif tag == "div" and self.section == "in-tree-files":
            entry = {"path": None, "license": None, "copyrights": [], "exceptions": []}
            (self.open[-1]["exceptions"] if self.open else self.files).append(entry)
            self.open.append(entry)
        elif tag == "p":
            self.text = []

    def handle_endtag(self, tag):
        if self.section == "out-of-tree-dependencies":
            return
        if tag == "div" and self.section == "in-tree-files":
            self.open.pop()
        elif tag == "p":
            self.paragraph()

    def handle_data(self, data):
        if self.text is not None:
            self.text.append(data)

    def paragraph(self):
        if self.text is None:
            return
        text, self.text = " ".join("".join(self.text).split()), None
        label, _, value = text.partition(":")
        value = value.strip()
        if self.section == "longer-version":
            self.statement.append(text)
        elif self.section == "in-tree-files" and self.open:
            entry = self.open[-1]
            if label == "File/Directory":
                entry["path"] = value
            elif label == "License":
                entry["license"] = value
            elif label == "Copyright":
                entry["copyrights"].append(value)
            elif label != "Exceptions" or value:
                raise ValueError(f"the standard library notice has {text!r}")


def library_files(entry):
    if not entry["path"] or not entry["license"]:
        raise ValueError("a standard library in-tree entry names no path or license")
    return LibraryFiles(
        path=entry["path"],
        license=entry["license"],
        copyrights=tuple(entry["copyrights"]),
        exceptions=tuple(library_files(exception) for exception in entry["exceptions"]),
    )


def walk(files):
    for entry in files:
        yield entry
        yield from walk(entry.exceptions)


def identifiers(node):
    """Return every license identifier that an expression node names."""
    kind, value = node
    if kind == "LICENSE":
        return {value}
    return set().union(*(identifiers(part) for part in value))


def read_library_notice(documents):
    """Read a toolchain's standard library notice and the license texts that
    it names; return its statement, its in-tree entries and those texts."""
    reader = LibraryNotice()
    reader.feed(read_text(documents / LIBRARY_NOTICE))
    reader.close()
    if tuple(reader.sections) != LIBRARY_SECTIONS or reader.open:
        expected = ", ".join(section for section in LIBRARY_SECTIONS if section)
        raise ValueError(
            f"{LIBRARY_NOTICE}: expected a table of contents and the sections "
            f"{expected}; review the reader before relying on this notice"
        )
    if not (reader.statement and reader.files):
        raise ValueError(f"{LIBRARY_NOTICE}: names no statement or files")
    files = tuple(library_files(entry) for entry in reader.files)
    named = set().union(
        *(identifiers(parse_expression(entry.license)) for entry in walk(files))
    )
    licenses = tuple(
        (license, read_text(documents / LIBRARY_LICENSES / f"{license}.txt"))
        for license in sorted(named)
    )
    return tuple(reader.statement), files, licenses


def target_platform(target):
    """The cfg options that the pinned toolchain's rustc sets for a target."""
    printed = subprocess.check_output(
        ["rustc", "--print", "cfg", "--target", target], cwd=WORKSPACE, text=True
    )
    options = set()
    for line in printed.splitlines():
        match = CFG_OPTION.fullmatch(line)
        if match is None:
            raise ValueError(f"rustc printed the cfg option {line!r} for {target}")
        options.add(match[1] if match[2] is None else (match[1], match[2]))
    return Platform(target, frozenset(options))


def applies(specification, platform):
    """Whether a manifest's [target.<specification>] table applies to a
    platform: the specification is a target name or a cfg() expression."""
    if not specification.startswith("cfg("):
        return specification == platform.target
    tokens, position = [], 0
    while position < len(specification.rstrip()):
        match = CFG_TOKEN.match(specification, position)
        if match is None:
            raise ValueError(f"malformed cfg expression: {specification!r}")
        tokens.append(match.groups())
        position = match.end()
    index = 0

    def take(kind, value=None):
        """Consume the next token, which must be a name (kind 0), a string
        (1) or the punctuation (2) `value`."""
        nonlocal index
        token = tokens[index] if index < len(tokens) else (None, None, None)
        if token[kind] is None or value not in (None, token[kind]):
            raise ValueError(f"malformed cfg expression: {specification!r}")
        index += 1
        return token[kind]

    def next_is(punctuation):
        return index < len(tokens) and tokens[index][2] == punctuation

    def predicate():
        name = take(0)
        if name in ("all", "any", "not") and next_is("("):
            take(2, "(")
            values = []
            while not next_is(")"):
                values.append(predicate())
                if not next_is(")"):
                    take(2, ",")
            take(2, ")")
            if name == "all":
                return all(values)
            if name == "any":
                return any(values)
            if len(values) != 1:
                raise ValueError(f"malformed cfg expression: {specification!r}")
            return not values[0]
        if next_is("="):
            take(2, "=")
            return (name, take(1)) in platform.options
        return name in platform.options

    take(0, "cfg")
    take(2, "(")
    matched = predicate()
    take(2, ")")
    if index != len(tokens):
        raise ValueError(f"malformed cfg expression: {specification!r}")
    return matched


def declared(manifest, platform=None):
    """Yield the key and table of each normal dependency that a manifest
    declares for a platform or, without one, for any platform."""
    tables = [manifest.get("dependencies", {})]
    for specification, table in manifest.get("target", {}).items():
        if platform is None or applies(specification, platform):
            tables.append(table.get("dependencies", {}))
    for table in tables:
        for key, value in table.items():
            yield key, value if isinstance(value, dict) else {"version": value}


def enabled_features(unit, wanted):
    """Expand the features that a package of the standard library is built
    with, as Cargo does. Return the features, the optional dependencies that
    they enable, and the features that they ask of each dependency, those of
    weak `dependency?/feature` items apart, which apply only to a dependency
    that is enabled anyway."""
    table = unit.manifest.get("features", {})
    optional = {key for key, spec in declared(unit.manifest) if spec.get("optional")}
    explicit = {
        item.removeprefix("dep:")
        for items in table.values()
        for item in items
        if item.startswith("dep:")
    }
    features, dependencies, asked, weak = set(), set(), {}, {}
    pending = list(wanted)
    while pending:
        feature = pending.pop()
        if feature in features:
            continue
        features.add(feature)
        if feature not in table:
            # An optional dependency is a feature of its own unless a `dep:`
            # item names it; a package without a default feature has none.
            if feature in optional and feature not in explicit:
                dependencies.add(feature)
            elif feature != "default":
                raise ValueError(f"{unit.label}: declares no feature {feature}")
            continue
        for item in table[feature]:
            key, slash, child = item.partition("/")
            if item.startswith("dep:"):
                dependencies.add(item.removeprefix("dep:"))
            elif not slash:
                pending.append(item)
            elif key.endswith("?"):
                weak.setdefault(key.removesuffix("?"), set()).add(child)
            else:
                asked.setdefault(key, set()).add(child)
                if key in optional:
                    dependencies.add(key)
    return features, dependencies, asked, weak


def library_crates(sources, platforms):
    """The notices of the out-of-tree crates that std links on the platforms,
    from the standard library's sources, a rust-src component's library
    directory, chosen like a locked crate's from the package in vendor/,
    and of the in-tree crates of LIBRARY_IN_TREE that it links, chosen from
    the files that the table names.

    library/Cargo.lock covers every platform and every feature of the
    workspace, so the crates follow the manifests: from std, built with
    LIBRARY_FEATURES, each normal dependency declared for a platform, an
    optional one only when a feature enables it. Features unify across
    platforms, as under the workspace's version 1 resolver. Each dependency is
    the package that the lock locks for its dependent; anything else that the
    sources do not settle stops generation.
    """
    lock = tomllib.loads(read_text(sources / "Cargo.lock"))
    workspace = tomllib.loads(read_text(sources / "Cargo.toml"))
    patches = workspace.get("patch", {}).get("crates-io", {})
    locked = {}
    for package in lock.get("package", []):
        locked.setdefault(package["name"], []).append(package)
    units = {}

    def unit(package, directory):
        identity = (package["name"], package["version"])
        if identity not in units:
            manifest = tomllib.loads(read_text(directory / "Cargo.toml"))
            named = manifest.get("package", {})
            if (named.get("name"), named.get("version")) != identity:
                raise ValueError(
                    f"{directory.relative_to(sources).as_posix()}: does not hold "
                    f"{identity[0]} {identity[1]}, which library/Cargo.lock locks"
                )
            units[identity] = LibraryUnit(package, directory, manifest)
        return identity

    def dependency(parent, key, table):
        """The package that a dependency of a package resolves to."""
        name = table.get("package", key)
        # The lock names a dependency alone, or with its version when it locks
        # several packages of that name.
        versions = [
            (entry.split(" ") + [None])[1]
            for entry in parent.locked.get("dependencies", [])
            if entry.split(" ")[0] == name
        ]
        candidates = [
            package
            for package in locked.get(name, [])
            if len(versions) == 1 and versions[0] in (None, package["version"])
        ]
        if len(candidates) != 1:
            raise ValueError(
                f"{parent.label}: library/Cargo.lock locks no one {name} for it"
            )
        package = candidates[0]
        if "source" in package:
            if package["source"] != CRATES_IO:
                raise ValueError(f"{name}: unsupported source {package['source']}")
            directory = sources / "vendor" / f"{name}-{package['version']}"
        elif "path" in table:
            directory = Path(os.path.normpath(parent.directory / table["path"]))
        elif "path" in patches.get(name, {}):
            directory = Path(os.path.normpath(sources / patches[name]["path"]))
        else:
            raise ValueError(f"{parent.label}: names no directory for {name}")
        return unit(package, directory)

    roots = [package for package in locked.get("std", []) if "source" not in package]
    if len(roots) != 1:
        raise ValueError("library/Cargo.lock must lock the in-tree std once")
    root = unit(roots[0], sources / "std")
    # The features of each package, unified across platforms, and the optional
    # dependencies that they enable.
    features, enabled = {}, {}
    pending = [(root, frozenset(LIBRARY_FEATURES))]
    while pending:
        identity, wanted = pending.pop()
        if identity in features and wanted <= features[identity]:
            continue
        current = units[identity]
        names, dependencies, asked, weak = enabled_features(
            current, features.get(identity, set()) | wanted
        )
        features[identity], enabled[identity] = names, dependencies
        for key, table in declared(current.manifest):
            if table.get("optional") and key not in dependencies:
                continue
            requested = set(table.get("features", ()))
            if table.get("default-features", table.get("default_features", True)):
                requested.add("default")
            requested |= asked.get(key, set()) | weak.get(key, set())
            pending.append((dependency(current, key, table), frozenset(requested)))
    reached = set()
    for platform in platforms:
        seen, pending = set(), [root]
        while pending:
            identity = pending.pop()
            if identity in seen:
                continue
            seen.add(identity)
            current = units[identity]
            for key, table in declared(current.manifest, platform):
                if not table.get("optional") or key in enabled[identity]:
                    pending.append(dependency(current, key, table))
        reached |= seen
    linked = {identity for identity in reached if "source" in units[identity].locked}
    notices = [
        in_tree_notice(sources, units[identity])
        for identity in sorted(reached - linked)
        if in_tree_license(units[identity])
    ]
    stale = set(LIBRARY_IN_TREE) - {notice.name for notice in notices}
    if stale:
        raise ValueError(
            f"LIBRARY_IN_TREE lists {sorted(stale)[0]}, which std does not link"
        )
    for name, version in sorted(linked):
        current = units[(name, version)]
        package = {
            "name": name,
            "version": version,
            "license": current.manifest["package"].get("license"),
            "manifest_path": str(current.directory / "Cargo.toml"),
            "source": CRATES_IO,
        }
        try:
            notices.append(crate_notice(package))
        except ValueError as error:
            raise ValueError(
                f"{error} (the standard library's {LIBRARY_SOURCES})"
            ) from error
    return tuple(
        sorted(notices, key=lambda notice: (notice.name, version_key(notice.version)))
    )


def in_tree_license(unit):
    """Whether an in-tree crate of the standard library declares a license
    that the notice does not give the library's files; one that does stops
    generation unless LIBRARY_IN_TREE lists it."""
    declared = unit.manifest["package"].get("license")
    if declared is None:
        own = False
    else:
        node = parse_expression(declared)
        own = node[0] != "OR" or frozenset(node[1]) != LIBRARY_STATEMENT
    if own != (unit.locked["name"] in LIBRARY_IN_TREE):
        raise ValueError(
            f"{unit.label}: the in-tree crate declares {declared!r}; list "
            "in LIBRARY_IN_TREE exactly the in-tree crates that std links and "
            "that declare another license than MIT OR Apache-2.0"
        )
    return own


def in_tree_notice(sources, unit):
    """The notice of an in-tree crate of LIBRARY_IN_TREE, chosen like a
    crate's from the license files that the table names for it."""
    name, version = unit.locked["name"], unit.locked["version"]
    texts = {
        relative: read_text(sources / relative) for relative in LIBRARY_IN_TREE[name]
    }
    files = [(relative, None) for relative in texts]
    declared = unit.manifest["package"]["license"]
    relied = relied_texts(unit.label, declared, files, texts)
    directory = unit.directory.relative_to(sources).as_posix()
    return Notice(name, version, f"in-tree, {directory}", *relied)


def read_standard_library(documents, sources, release, platforms):
    """Read a toolchain's standard library: the statement, in-tree entries and
    license texts of its notice below `documents`, and the crates that std
    links on the platforms, from its sources."""
    statement, files, licenses = read_library_notice(documents)
    crates = library_crates(sources, platforms)
    return StandardLibrary(release, statement, files, licenses, crates)


def standard_library():
    """Read the standard library of the toolchain that builds the packages.

    Release builds link the standard library of the channel that
    rust/rust-toolchain.toml pins, which rustup selects inside rust/. Its
    sources are its rust-src component, below the sysroot unless the variable
    that LIBRARY_SOURCES_VARIABLE names gives an unpacked copy's.
    """
    channel = toolchain.channel()
    described = subprocess.check_output(["rustc", "-vV"], cwd=WORKSPACE, text=True)
    release = re.search(r"(?m)^release: (\S+)$", described)
    if release is None or release[1] != channel:
        found = release[1] if release else "of an unknown release"
        raise ValueError(
            f"rust/: rustc is {found}, but the notices need Rust {channel}, which "
            "rust/rust-toolchain.toml pins for the released packages"
        )
    sysroot = Path(
        subprocess.check_output(
            ["rustc", "--print", "sysroot"], cwd=WORKSPACE, text=True
        ).strip()
    )
    named = os.environ.get(LIBRARY_SOURCES_VARIABLE)
    sources = Path(named) if named else sysroot / LIBRARY_SOURCES
    if not (sources / "Cargo.lock").is_file():
        raise ValueError(
            f"{sources}: holds no standard library sources; add the rust-src "
            f"component to Rust {channel} (`rustup component add rust-src` in "
            f"rust/), or set {LIBRARY_SOURCES_VARIABLE} to the {LIBRARY_SOURCES} "
            "directory of an unpacked copy of it"
        )
    platforms = [target_platform(target) for target in TARGETS]
    return read_standard_library(
        sysroot / LIBRARY_DOCUMENTS, sources, channel, platforms
    )


def wrapped(prefix, units, indent=""):
    """Wrap a comma-separated list at unit boundaries, never inside a unit."""
    pieces = [f"{unit}," for unit in units[:-1]] + list(units[-1:])
    lines, line = [], indent + prefix
    for piece in pieces:
        if len(line) + 1 + len(piece) > WIDTH:
            lines.append(line)
            line = indent + "    " + piece
        else:
            line = f"{line} {piece}"
    return lines + [line]


def paragraph(text):
    return textwrap.wrap(text, width=WIDTH, break_on_hyphens=False) + [""]


def library_tree(entry, indent=""):
    """Render a standard library in-tree entry with its nested exceptions."""
    lines = [f"{indent}File/Directory: {entry.path}"]
    lines.append(f"{indent}    License: {entry.license}")
    lines += [f"{indent}    Copyright: {holder}" for holder in entry.copyrights]
    if entry.exceptions:
        lines.append(f"{indent}    Exceptions:")
        for exception in entry.exceptions:
            lines += library_tree(exception, indent + "        ")
    return lines


def build_features(host):
    """Name the features that a host's release build enables, as its notices
    say it."""
    features = BUILD_FEATURES[host]
    if not features:
        return "its default features"
    named = ", ".join(features)
    return f"its default features and the feature{'s' * (len(features) > 1)} {named}"


def render(notices, library, host):
    """Render a host's notices file from the texts of its crates and the
    standard library."""
    # Shipped texts group by content; canonical texts group by license.
    blocks = {}

    def number(key, text, holder):
        block = blocks.setdefault(
            key, {"index": len(blocks) + 1, "text": text, "holders": []}
        )
        # A crate that the standard library shares with the locked graph is
        # named once.
        if holder not in block["holders"]:
            block["holders"].append(holder)
        return block["index"]

    def entry(notice, missing):
        """A crate's entry: its one license and the numbered texts for it."""
        crate = f"{notice.name} {notice.version}"
        references = [
            f"[{number(text, text, f'{crate} ({relative})')}] {relative}"
            for relative, text in notice.texts
        ]
        for license in notice.fallbacks:
            index = number(("canonical", license), canonical_text(license), crate)
            references.append(f"[{index}] canonical {license} text; {missing}")
        lines = [f"Crate: {crate}"]
        if notice.source is not None:
            lines.append(f"    Source: {notice.source}")
        lines.append(f"    License: {' AND '.join(notice.relied)}")
        return lines + wrapped("Texts:", references, indent="    ") + [""]

    entries = []
    for notice in notices:
        entries += entry(notice, "the crate package ships none")

    release = library.release
    notice = f"{LIBRARY_DOCUMENTS}/{LIBRARY_NOTICE}"
    standard = [f"From {notice} of Rust {release}:", ""]
    for statement in library.statement:
        standard += paragraph(statement)
    for tree in library.files:
        standard += library_tree(tree) + [""]
    licenses = f"{LIBRARY_DOCUMENTS}/{LIBRARY_LICENSES}"
    references = [
        f"[{number(text, text, f'Rust {release} ({licenses}/{license}.txt)')}] {license}"
        for license, text in library.licenses
    ]
    standard += wrapped("License texts:", references) + [""]
    standard += paragraph(
        f"From the rust-src component of Rust {release}, {LIBRARY_SOURCES}, "
        "the crates that the standard library links, and those of its in-tree "
        "crates that declare a license of their own:"
    )
    for crate in library.crates:
        standard += entry(crate, "the crate package ships none")

    subject, destination = LINKED_BY[host]
    linked, notices_of = "third-party Rust crates and the Rust standard library", ""
    if host == "cli":
        # Only the archives for the musl targets carry musl's notice.
        linked += ", and on the musl targets the musl C library,"
        notices_of = (
            f" The archives for the musl targets also carry {MUSL_NOTICE}, the "
            "notice of musl."
        )
    lines = ["yamaa third-party notices", "=" * 25, ""]
    lines += paragraph(
        f"{subject} {linked} into {destination}. "
        "This file lists those crates with the license that yamaa relies on and "
        "the license, copyright and NOTICE texts that each crate ships for it, "
        "and the standard library's licensing and the crates that it links. "
        f"The project's own license is in {PROJECT_LICENSE}; LICENSE-READSTAT.txt, "
        "LICENSE-UNICODE.txt and LICENSE-YAML.txt carry the notices for the "
        "vendored ReadStat sources, the Unicode data in yamaa's own tables and "
        f"the vendored saphyr-parser crate.{notices_of}"
    )
    lines += paragraph(
        "The list covers the third-party crates that the shipped crate "
        f"{engine_bundle.BINDINGS[host]} links, as locked in rust/Cargo.lock and "
        f"built with {build_features(host)}: its normal dependency closure on "
        f"each of the targets {', '.join(TARGETS)}, without procedural macros. "
        "Crates that only build scripts or procedural macros use, and crates "
        "that only other features or other platforms select, are not linked, so "
        "they are not listed. Crates come from crates.io unless an entry names "
        "another source."
    )
    lines += paragraph(
        "Where a crate offers a choice of licenses, yamaa relies on the first "
        "license in this order that the crate offers and whose text the crate "
        f"ships: {', '.join(ACCEPTED)}. Each entry names only the license relied "
        "on. Texts that only grant another license are not reproduced; the "
        "crate's general license, copyright, NOTICE and AUTHORS files are."
    )
    lines += paragraph(
        "Texts are reproduced verbatim, with LF line endings and a final "
        "newline. Identical texts are printed once and numbered; each entry "
        "names the numbered texts its crate ships. A crate whose package ships "
        "no notice, but whose repository also publishes a listed crate that "
        "does, names that crate's texts. Otherwise, where a crate package ships "
        "no text for the license it is relied on under, the entry says so and "
        f"the canonical text from the SPDX License List {SPDX_RELEASE} is "
        "reproduced instead, with its placeholders unfilled."
    )
    lines += paragraph(
        "The Rust standard library section gives the Rust project's licensing "
        "statement and the licenses and copyright holders of the standard "
        "library's own files, with the license texts that the toolchain ships in "
        f"{LIBRARY_DOCUMENTS}/{LIBRARY_LICENSES}, from the notice that Rust "
        f"{release}, the toolchain that builds the released packages, ships as "
        f"{notice}. It then lists the third-party crates that the standard "
        "library links on the targets above, from the toolchain's rust-src "
        "component: those that library/std/Cargo.toml declares for the targets "
        "and for the features that the Rust project builds std with, and their "
        "dependencies, at the versions that library/Cargo.lock locks, each with "
        "the texts that its package in library/vendor ships, chosen like a "
        "crate's, and the in-tree crates that the standard library links whose "
        "manifests declare a license that the notice does not give them, with "
        "the license files of their sources."
    )
    lines += paragraph(
        "This file is generated by tools/packaging/generate_third_party_notices.py "
        f"from rust/Cargo.lock and the Rust {release} toolchain; do not edit it "
        "by hand."
    )
    lines += ["Crates", "-" * 6, ""] + entries
    lines += ["Rust standard library", "-" * 21, ""] + standard
    if host == "cli":
        lines += ["musl C library", "-" * 14, ""]
        lines += paragraph(
            f"The yamaa executables for {' and '.join(MUSL_TARGETS)} link musl, "
            f"the C library that Rust {release} bundles for those targets, "
            f"statically. Their archives carry its notice as {MUSL_NOTICE}: "
            "musl's COPYRIGHT file, verbatim, after a header that names the "
            "release."
        )
        lines += [
            f"Library: musl {musl_release(release)}",
            "    License: MIT",
            f"    Text: {MUSL_NOTICE}",
            "",
        ]
    lines += ["Texts", "-" * 5, ""]
    for key, block in blocks.items():
        if isinstance(key, tuple):
            prefix = (
                f"[{block['index']}] Canonical {key[1]} text from the SPDX License "
                f"List {SPDX_RELEASE}, for:"
            )
        else:
            prefix = f"[{block['index']}] Shipped by:"
        lines += ["=" * WIDTH, *wrapped(prefix, block["holders"]), "=" * WIDTH, ""]
        # Each text ends with exactly the newline that the join restores.
        lines += [block["text"][:-1], ""]
    return refuse_gnu("\n".join(lines), f"the {host} notices")


def lender(package, packages):
    """Return the crate whose texts a SHARED_TEXTS crate reproduces, if any."""
    name = SHARED_TEXTS.get(package["name"])
    if name is None:
        return None
    repository = package.get("repository")
    lenders = [
        other
        for other in packages
        if other["name"] == name
        and repository
        and other.get("repository") == repository
    ]
    if len(lenders) != 1:
        raise ValueError(
            f"{package['name']} {package['version']}: {name} must ship once, from "
            "the same repository, to lend it its texts"
        )
    return lenders[0]


def notices_for(metadata, linked):
    """The notices of the third-party crates among the IDs of a shipped
    crate's linked packages."""
    packages = third_party(metadata, linked)
    return [crate_notice(package, lender(package, packages)) for package in packages]


def relied_on(package, metadata):
    """The license that yamaa relies on for a package of the locked graph, as
    an SPDX expression: a workspace member's own license, and for any other
    crate the licenses that a notice entry for it names."""
    if package["id"] in metadata["workspace_members"]:
        relied = choose(parse_expression(package["license"]), frozenset(ACCEPTED))
        return " AND ".join(ordered(relied))
    notice = crate_notice(package, lender(package, metadata["packages"]))
    return " AND ".join(notice.relied)


def committed_entries(text):
    """Return the (name, version) pairs in a notices file's crate section."""
    crates = re.split(
        r"\n(?:Rust standard library|Texts)\n-+\n",
        text.partition("\nCrates\n------\n")[2],
    )[0]
    return {
        (match.group(1), match.group(2))
        for match in re.finditer(r"(?m)^Crate: (\S+) (\S+)$", crates)
    }


def license_expression(metadata, linked, notices, library):
    """Combine the licenses of the members that the Python binding links with
    those its linked crates and the crates that the standard library links
    rely on; `linked` holds the IDs of the packages that the Python binding
    links, `notices` are the Python host's and `library` is the standard
    library that the wheels link, whose compiler_builtins brings Apache-2.0
    WITH LLVM-exception (#3030 D185)."""
    packages = {package["id"]: package for package in metadata["packages"]}
    members = set(metadata["workspace_members"])
    licenses = set()
    for identity in linked & members:
        member = packages[identity]
        licenses |= choose(parse_expression(member["license"]), frozenset(ACCEPTED))
    relied = {(notice.name, notice.version): notice.relied for notice in notices}
    for identity in linked - members:
        package = packages[identity]
        licenses |= set(relied[(package["name"], package["version"])])
    for crate in library.crates:
        licenses |= set(crate.relied)
    return " AND ".join(ordered(licenses))


def python_project():
    return tomllib.loads((PYTHON / "pyproject.toml").read_text(encoding="utf-8"))[
        "project"
    ]


def check_copies(project):
    """Require the package copies of the project license and notices to agree.

    The hosts' copies of HOST_NOTICES differ; their generators check them. The
    command line keeps no copy of the other notices, only its musl notice
    (check_musl_notice).
    """
    errors = []
    names = project["license-files"]
    for required in (PROJECT_LICENSE, NOTICES):
        if required not in names:
            errors.append(f"python/pyproject.toml: license-files must list {required}")
    for name in names:
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", name):
            errors.append(f"python/pyproject.toml: license file {name!r} is not a name")
            continue
        python_copy, r_copy = PYTHON / name, R_INST / name
        if not python_copy.is_file():
            errors.append(f"python/{name}: listed in license-files but missing")
        elif name not in HOST_NOTICES and (
            not r_copy.is_file() or r_copy.read_bytes() != python_copy.read_bytes()
        ):
            errors.append(f"R/yamaa/inst/{name}: must be identical to python/{name}")
    for directory, relative in ((PYTHON, "python"), (R_INST, "R/yamaa/inst")):
        for path in sorted(directory.glob("LICENSE*")):
            if path.name not in names:
                errors.append(
                    f"{relative}/{path.name}: not a package notice in license-files"
                )
    # The command-line archives carry the Python packages' copies of the
    # shared notices, so a copy beside the crate would go unused.
    for path in sorted(CLI.glob("LICENSE*")):
        if path.name not in CLI_NOTICES:
            errors.append(
                f"{CLI.relative_to(ROOT).as_posix()}/{path.name}: the command "
                f"line keeps only its own {NOTICES}, {INVENTORY} and {MUSL_NOTICE}"
            )
    project_license = (ROOT / "LICENSE").read_bytes()
    for directory, relative in ((PYTHON, "python"), (R_INST, "R/yamaa/inst")):
        path = directory / PROJECT_LICENSE
        if path.is_file() and path.read_bytes() != project_license:
            errors.append(f"{relative}/{PROJECT_LICENSE}: must be a copy of LICENSE")
    return errors


def musl_release(release):
    """The musl release that the musl targets of Rust `release` bundle; stop
    unless MUSL was reviewed for that toolchain."""
    reviewed, musl = MUSL
    if release != reviewed:
        raise ValueError(
            f"the musl notice was reviewed for Rust {reviewed}, not {release}: set "
            f"MUSL to the musl release that Rust {release} bundles for "
            f"{' and '.join(MUSL_TARGETS)}, and {MUSL_NOTICE} to its COPYRIGHT file"
        )
    return musl


def check_musl_notice(release):
    """Require the command line's musl notice to name the musl release that
    Rust `release` bundles and to hold the MIT license, and no GNU license."""
    path = CLI / MUSL_NOTICE
    relative = path.relative_to(ROOT).as_posix()
    if not path.is_file():
        return [f"{relative}: missing; the archives for the musl targets carry it"]
    text = read_text(path)
    header = f"musl {musl_release(release)}\n{MUSL_HOME}\n\n"
    if not text.startswith(header):
        return [f"{relative}: must start with the header {header!r}"]
    if "MIT" not in detect(text):
        return [f"{relative}: lacks musl's MIT license"]
    found = gnu_licenses(text)
    return [f"{relative}: names a GNU license ({found[0]})"] if found else []


def check(metadata, closures, library):
    """Return each host's notices and every finding for the committed notices
    and license copies; `closures` maps each host to the IDs of the packages
    that its shipped crate links (linked_packages)."""
    selected, errors = {}, []
    for host, directory in copies().items():
        crate = engine_bundle.BINDINGS[host]
        selected[host] = notices_for(metadata, closures[host])
        expected = render(selected[host], library, host).encode("utf-8")
        linked = {(notice.name, notice.version) for notice in selected[host]}
        path = directory / NOTICES
        relative = path.relative_to(ROOT).as_posix()
        if not path.is_file():
            errors.append(f"{relative}: missing")
            continue
        committed = path.read_bytes()
        listed = committed_entries(committed.decode("utf-8"))
        for name, version in sorted(linked - listed):
            errors.append(
                f"{relative}: no entry for {name} {version}, which {crate} links"
            )
        for name, version in sorted(listed - linked):
            errors.append(
                f"{relative}: lists {name} {version}, which {crate} does not link"
            )
        if committed != expected:
            errors.append(
                f"{relative}: differs from the notices generated for {crate} from "
                f"rust/Cargo.lock and the Rust {library.release} toolchain"
            )
    project = python_project()
    errors += check_copies(project)
    errors += check_musl_notice(library.release)
    expression = license_expression(
        metadata, closures["python"], selected["python"], library
    )
    if project["license"] != expression:
        errors.append(
            f"python/pyproject.toml: license must be {expression!r} to cover the "
            f"linked crates, not {project['license']!r}"
        )
    return selected, errors


def write(metadata, closures, library):
    """Write each host's notices and project license copy; return each host's
    notices."""
    project_license = (ROOT / "LICENSE").read_bytes()
    selected = {}
    for host, directory in copies().items():
        selected[host] = notices_for(metadata, closures[host])
        payload = render(selected[host], library, host).encode("utf-8")
        (directory / NOTICES).write_bytes(payload)
        # The command line's archives carry the Python packages' copy.
        if host != "cli":
            (directory / PROJECT_LICENSE).write_bytes(project_license)
    return selected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="regenerate the notices and the project license copies",
    )
    args = parser.parse_args()
    errors = []
    try:
        metadata = locked_metadata()
        closures = linked_packages(metadata)
        library = standard_library()
        if args.write:
            selected = write(metadata, closures, library)
        else:
            selected, errors = check(metadata, closures, library)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")
    if errors:
        parser.exit(
            1,
            "\n".join(errors)
            + "\nRun: python -m tools.packaging.generate_third_party_notices --write\n",
        )
    for host, directory in copies().items():
        print(
            f"{'Wrote ' if args.write else ''}"
            f"{(directory / NOTICES).relative_to(ROOT).as_posix()}: the "
            f"{len(selected[host])} third-party crates of "
            f"{engine_bundle.BINDINGS[host]} and the Rust {library.release} "
            "standard library"
        )


if __name__ == "__main__":
    main()
