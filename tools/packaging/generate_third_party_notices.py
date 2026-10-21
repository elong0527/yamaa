"""Generate the license notices for the third-party code in the native packages.

The yamaa wheels, the installed R package and the command-line binaries
statically link Rust crates from the locked graph. Each host's
LICENSE-THIRD-PARTY.txt lists the third-party crates that the host's shipped
crate links: its normal dependency closure on each of the supported TARGETS,
without procedural macros, which run inside the compiler. Crates that only
build scripts or procedural macros use, and the crates of other platforms,
are not linked and are not listed; the source packages carry their license
files with their vendored sources. python/ holds the copy of the Python
packages, which covers yamaa-python, R/yamaa/inst/ the copy of the R package,
which covers yamaa-r, and rust/crates/yamaa-cli/ the copy of the command-line
archives, which covers yamaa-cli. Each entry names the one license that yamaa
relies on, with the license, copyright and NOTICE texts that the crate ships
for it. The inventory comes from `cargo metadata --locked` for rust/Cargo.lock
and the texts from the crate packages Cargo unpacks.

The native code also links the Rust standard library of the toolchain that
rust/rust-toolchain.toml pins. Its notices come from the notice that the
Rust project ships with that toolchain, share/doc/rust/COPYRIGHT-library.html,
and the license texts beside it, of whose out-of-tree crates they keep those
that LIBRARY_LINKED names. No license text is looked up anywhere else.

No notice names a GNU license: generation stops when a reproduced text would
(gnu_licenses), apart from the clause of the LLVM exception, an accepted
license, that names GPLv2.

By default this checks the committed copies against the locked graph and the
pinned toolchain, the package copies of the project license and notices
against their sources, and the Python License-Expression against the licenses
that the crates linked into the Python binding are relied on under. `--write`
regenerates the notices and the license copies.
"""

import argparse
import json
import re
import subprocess
import textwrap
import tomllib
from dataclasses import dataclass
from html.parser import HTMLParser
from pathlib import Path

from tools.ci import toolchain
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
# The Python project lists the package notices; the R package installs its
# copies from inst/. The command line keeps only its HOST_NOTICES beside its
# crate: its archives carry the Python packages' copies of the other notices
# (tools/packaging/cli_archive.py).
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
SPDX_TEXTS = Path(__file__).with_name("spdx")
SPDX_RELEASE = "3.29.0"
WIDTH = 78

# Licenses a shipped crate may be relied on under, in preference order. Where a
# crate offers a choice, the first offered license whose text the crate ships
# is relied on. A crate usable only under another license stops generation
# until its license has been reviewed and added here. Zlib is zlib-rs's only
# license: Parquet 55 selects that flate2 backend.
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
# Another shape stops generation.
LIBRARY_SECTIONS = (
    None,
    "short-version",
    "longer-version",
    "in-tree-files",
    "out-of-tree-dependencies",
)
# The out-of-tree crates of the standard library notice that the standard
# library links on TARGETS, by name and version. The notice also names the
# crates of the compiler's tools, of the test harness and of other targets,
# such as r-efi for UEFI, which the packages never link. Review this table
# with every toolchain update, against the dependencies that
# library/std/Cargo.toml in the rust-src component declares for TARGETS, at
# the versions that library/Cargo.lock locks. The standard library of Rust
# 1.99.1 also links addr2line, adler2, miniz_oxide, object and memchr 2.8.3,
# which its notice does not name; memchr 2.8.0, which it names, stands for
# the last.
LIBRARY_LINKED = (
    ("cfg-if", "1.0.4"),
    ("gimli", "0.34.0"),
    ("hashbrown", "0.17.1"),
    ("libc", "0.2.189"),
    ("memchr", "2.8.0"),
    ("rustc-demangle", "0.1.28"),
)


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
class LibraryCrate:
    """An out-of-tree crate of the standard library, with its published notices."""

    name: str
    version: str
    declared: str
    texts: tuple[tuple[str, str], ...]


@dataclass(frozen=True)
class StandardLibrary:
    release: str
    statement: tuple[str, ...]
    files: tuple[LibraryFiles, ...]
    licenses: tuple[tuple[str, str], ...]
    crates: tuple[LibraryCrate, ...]


def locked_metadata(target=None):
    """Resolve the locked graph with every feature of every workspace member,
    for every platform or, given a target, for that target alone."""
    command = ["cargo", "metadata", "--locked", "--all-features"]
    command += ["--format-version", "1"]
    if target is not None:
        command += ["--filter-platform", target]
    return json.loads(subprocess.check_output(command, cwd=WORKSPACE))


def linked_metadata():
    """The locked graph of each of TARGETS, by target."""
    return {target: locked_metadata(target) for target in TARGETS}


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


def dependency_closure(metadata, crate):
    """Return the package IDs that a shipped crate links, itself included.

    The closure follows normal edges and stops at procedural macros, which run
    inside the compiler instead of the shipped binaries; build scripts and
    their dependencies run only while compiling. A graph that `cargo metadata
    --filter-platform` resolved for one target holds only that target's edges.
    """
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [
        identity
        for identity in metadata["workspace_members"]
        if packages[identity]["name"] == crate
    ]
    if len(roots) != 1:
        raise ValueError(f"the workspace must contain the shipped crate {crate} once")
    seen, pending = set(), roots
    while pending:
        identity = pending.pop()
        if identity in seen:
            continue
        seen.add(identity)
        for dependency in nodes[identity]["deps"]:
            kinds = {kind["kind"] for kind in dependency["dep_kinds"]}
            target = packages[dependency["pkg"]]
            if None in kinds and not any(
                "proc-macro" in item["kind"] for item in target["targets"]
            ):
                pending.append(dependency["pkg"])
    return seen


def linked_closure(graphs, crate):
    """Return the package IDs that a shipped crate links on any target;
    `graphs` maps each of TARGETS to its locked graph."""
    return set().union(*(dependency_closure(graph, crate) for graph in graphs.values()))


def third_party(graphs, crate):
    """Return the third-party packages that a shipped crate links on any
    target, in a stable order."""
    packages = {
        package["id"]: package
        for graph in graphs.values()
        for package in graph["packages"]
    }
    members = set().union(*(graph["workspace_members"] for graph in graphs.values()))
    selected = [
        packages[identity]
        for identity in linked_closure(graphs, crate)
        if identity not in members
    ]
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
    """Read the Rust project's standard library notice into its parts.

    The notice is the HTML that the Rust project's generate-copyright tool
    renders: the licensing statement, nested in-tree entries, and one heading
    per out-of-tree crate with its notice texts in <pre> blocks. A line outside
    that shape stops generation instead of being skipped.
    """

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.sections, self.statement, self.files, self.crates = [], [], [], []
        self.open = []  # in-tree entries being read, outermost first
        self.text = self.pre = self.name = None

    @property
    def section(self):
        return self.sections[-1] if self.sections else None

    def handle_starttag(self, tag, attrs):
        if tag == "h2":
            self.sections.append(dict(attrs).get("id"))
        elif tag == "div" and self.section == "in-tree-files":
            entry = {"path": None, "license": None, "copyrights": [], "exceptions": []}
            (self.open[-1]["exceptions"] if self.open else self.files).append(entry)
            self.open.append(entry)
        elif tag in ("h3", "p", "summary"):
            self.text = []
        elif tag == "details":
            # The paragraph that names the notices encloses them.
            self.paragraph()
        elif tag == "pre":
            self.pre = []

    def handle_endtag(self, tag):
        if tag == "div" and self.section == "in-tree-files":
            self.open.pop()
        elif tag == "h3":
            title, self.text = " ".join("".join(self.text).split()), None
            self.crates.append(
                {"title": title, "url": None, "license": None, "texts": []}
            )
        elif tag == "p":
            self.paragraph()
        elif tag == "summary":
            self.name, self.text = "".join(self.text).strip(), None
        elif tag == "pre":
            # The template wraps each text in a newline and an indented line.
            match = re.fullmatch(r"\n(.*)\n *", "".join(self.pre), re.DOTALL)
            if match is None or self.name is None or not self.crates:
                raise ValueError("the standard library notice has an unexpected text")
            self.crates[-1]["texts"].append((self.name, normalized(match[1])))
            self.pre = self.name = None

    def handle_data(self, data):
        if self.pre is not None:
            self.pre.append(data)
        elif self.text is not None:
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
        elif self.section == "out-of-tree-dependencies" and self.crates:
            if label in ("URL", "License"):
                self.crates[-1][label.lower()] = value
            elif label not in ("Authors", "Notices"):
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


def read_standard_library(documents, release):
    """Read a toolchain's standard library notice and the license texts it names."""
    reader = LibraryNotice()
    reader.feed(read_text(documents / LIBRARY_NOTICE))
    reader.close()
    if tuple(reader.sections) != LIBRARY_SECTIONS or reader.open:
        expected = ", ".join(section for section in LIBRARY_SECTIONS if section)
        raise ValueError(
            f"{LIBRARY_NOTICE}: expected a table of contents and the sections "
            f"{expected}; review the reader before relying on this notice"
        )
    if not (reader.statement and reader.files and reader.crates):
        raise ValueError(f"{LIBRARY_NOTICE}: names no statement, files or crates")
    files = tuple(library_files(entry) for entry in reader.files)
    crates = []
    for crate in reader.crates:
        match = re.fullmatch(
            r"https://crates\.io/crates/([^/\s]+)/([^/\s]+)", crate["url"] or ""
        )
        if (
            match is None
            or crate["title"] != f"\N{PACKAGE} {match[1]}-{match[2]}"
            or not crate["license"]
        ):
            raise ValueError(f"{LIBRARY_NOTICE}: unexpected crate {crate['title']!r}")
        crates.append(
            LibraryCrate(match[1], match[2], crate["license"], tuple(crate["texts"]))
        )
    named = set().union(
        *(identifiers(parse_expression(entry.license)) for entry in walk(files))
    )
    licenses = tuple(
        (license, read_text(documents / LIBRARY_LICENSES / f"{license}.txt"))
        for license in sorted(named)
    )
    return StandardLibrary(
        release=release,
        statement=tuple(reader.statement),
        files=files,
        licenses=licenses,
        crates=tuple(
            sorted(crates, key=lambda crate: (crate.name, version_key(crate.version)))
        ),
    )


def standard_library():
    """Read the standard library notice of the toolchain that builds the packages.

    Release builds link the standard library of the channel that
    rust/rust-toolchain.toml pins, which rustup selects inside rust/.
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
    sysroot = subprocess.check_output(
        ["rustc", "--print", "sysroot"], cwd=WORKSPACE, text=True
    ).strip()
    return read_standard_library(Path(sysroot) / LIBRARY_DOCUMENTS, channel)


def library_notices(library):
    """The notices of the standard library's out-of-tree crates that TARGETS
    link, as LIBRARY_LINKED names them, with the texts that the Rust project's
    notice reproduces for each, chosen like a locked crate's."""
    published = {(crate.name, crate.version): crate for crate in library.crates}
    selected = []
    for name, version in LIBRARY_LINKED:
        crate = published.get((name, version))
        label = f"{name} {version} of the Rust {library.release} standard library"
        if crate is None:
            raise ValueError(
                f"{label}: its notice does not name it; review LIBRARY_LINKED"
            )
        files = [(text, classify(text, label)) for text, _ in crate.texts]
        selected.append(
            Notice(
                name,
                version,
                None,
                *relied_texts(label, crate.declared, files, dict(crate.texts)),
            )
        )
    return selected


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
    for crate in library_notices(library):
        standard += entry(crate, "the Rust project's notice reproduces none")

    subject, destination = LINKED_BY[host]
    lines = ["yamaa third-party notices", "=" * 25, ""]
    lines += paragraph(
        f"{subject} third-party Rust crates and the Rust standard library into "
        f"{destination}. "
        "This file lists those crates with the license that yamaa relies on and "
        "the license, copyright and NOTICE texts that each crate ships for it, "
        "and the standard library's licensing from the Rust project's notice. "
        f"The project's own license is in {PROJECT_LICENSE}; LICENSE-READSTAT.txt, "
        "LICENSE-UNICODE.txt and LICENSE-YAML.txt carry the notices for the "
        "vendored ReadStat sources, the Unicode data in yamaa's own tables and "
        "the vendored saphyr-parser crate."
    )
    lines += paragraph(
        "The list covers the third-party crates that the shipped crate "
        f"{engine_bundle.BINDINGS[host]} links, as locked in rust/Cargo.lock: its "
        f"normal dependency closure on each of the targets {', '.join(TARGETS)}, "
        "without procedural macros. Crates that only build scripts or procedural "
        "macros use, and crates of other platforms, are not linked, so they are "
        "not listed. Crates come from crates.io unless an entry names another "
        "source."
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
        f"{notice}. That notice also names every out-of-tree crate used to build "
        "the standard library, for every target and for the tools that build it. "
        "This section lists those that the standard library links on the targets "
        "above, with the texts that the Rust project reproduces for each, chosen "
        "like a crate's."
    )
    lines += paragraph(
        "This file is generated by tools/packaging/generate_third_party_notices.py "
        f"from rust/Cargo.lock and the Rust {release} toolchain; do not edit it "
        "by hand."
    )
    lines += ["Crates", "-" * 6, ""] + entries
    lines += ["Rust standard library", "-" * 21, ""] + standard
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


def notices_for(graphs, crate):
    """The notices of the third-party crates that a shipped crate links."""
    packages = third_party(graphs, crate)
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


def license_expression(graphs, notices):
    """Combine the licenses of the members that the Python binding links with
    those its linked crates rely on; `notices` are the Python host's."""
    packages = {
        package["id"]: package
        for graph in graphs.values()
        for package in graph["packages"]
    }
    members = set().union(*(graph["workspace_members"] for graph in graphs.values()))
    linked = linked_closure(graphs, engine_bundle.BINDINGS["python"])
    licenses = set()
    for identity in linked & members:
        member = packages[identity]
        licenses |= choose(parse_expression(member["license"]), frozenset(ACCEPTED))
    relied = {(notice.name, notice.version): notice.relied for notice in notices}
    for identity in linked - members:
        package = packages[identity]
        licenses |= set(relied[(package["name"], package["version"])])
    return " AND ".join(ordered(licenses))


def python_project():
    return tomllib.loads((PYTHON / "pyproject.toml").read_text(encoding="utf-8"))[
        "project"
    ]


def check_copies(project):
    """Require the package copies of the project license and notices to agree.

    The hosts' copies of HOST_NOTICES differ; their generators check them. The
    command line keeps no copy of the other notices.
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
        if path.name not in HOST_NOTICES:
            errors.append(
                f"{CLI.relative_to(ROOT).as_posix()}/{path.name}: the command "
                f"line keeps only its own {NOTICES} and {INVENTORY}"
            )
    project_license = (ROOT / "LICENSE").read_bytes()
    for directory, relative in ((PYTHON, "python"), (R_INST, "R/yamaa/inst")):
        path = directory / PROJECT_LICENSE
        if path.is_file() and path.read_bytes() != project_license:
            errors.append(f"{relative}/{PROJECT_LICENSE}: must be a copy of LICENSE")
    return errors


def check(graphs, library):
    """Return each host's notices and every finding for the committed notices
    and license copies; `graphs` maps each of TARGETS to its locked graph."""
    selected, errors = {}, []
    for host, directory in copies().items():
        crate = engine_bundle.BINDINGS[host]
        selected[host] = notices_for(graphs, crate)
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
    expression = license_expression(graphs, selected["python"])
    if project["license"] != expression:
        errors.append(
            f"python/pyproject.toml: license must be {expression!r} to cover the "
            f"linked crates, not {project['license']!r}"
        )
    return selected, errors


def write(graphs, library):
    """Write each host's notices and project license copy; return each host's
    notices."""
    project_license = (ROOT / "LICENSE").read_bytes()
    selected = {}
    for host, directory in copies().items():
        selected[host] = notices_for(graphs, engine_bundle.BINDINGS[host])
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
        graphs = linked_metadata()
        library = standard_library()
        if args.write:
            selected = write(graphs, library)
        else:
            selected, errors = check(graphs, library)
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
