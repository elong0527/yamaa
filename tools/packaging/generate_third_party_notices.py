"""Generate the license notices for the third-party crates in the native packages.

The yamaa wheels and the installed R package statically link Rust crates from
the locked graph. LICENSE-THIRD-PARTY.txt lists every third-party crate in the
normal and build dependency closure of the shipped crates yamaa-python and
yamaa-r, for every target platform, with the license, copyright and NOTICE
texts that the crate ships. The inventory comes from `cargo metadata --locked`
for rust/Cargo.lock and the texts from the crate packages Cargo unpacks; no
license text is looked up anywhere else.

By default this checks the committed copies against the locked graph, the
package copies of the project license and notices against their sources, and
the Python License-Expression against the licenses that the linked crates are
relied on under. `--write` regenerates the notices and the license copies.
"""

import argparse
import json
import re
import subprocess
import textwrap
import tomllib
from dataclasses import dataclass
from pathlib import Path

from tools.paths import ROOT, WORKSPACE

NOTICES = "LICENSE-THIRD-PARTY.txt"
PROJECT_LICENSE = "LICENSE.txt"
# The Python project lists the package notices; the R package installs the
# same files from inst/.
PYTHON = ROOT / "python"
R_INST = ROOT / "R/yamaa/inst"
SHIPPED = ("yamaa-python", "yamaa-r")
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
SPDX_TEXTS = Path(__file__).with_name("spdx")
SPDX_RELEASE = "3.29.0"
WIDTH = 78

# Licenses a shipped crate may be relied on under, in preference order. Where a
# crate offers a choice, the first offered license whose text the crate ships
# is relied on. A crate usable only under another license stops generation
# until its license has been reviewed and added here.
ACCEPTED = (
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "Unicode-3.0",
    "Apache-2.0 WITH LLVM-exception",
    "CC0-1.0",
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
}

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
    "saphyr-parser": {
        "LICENSES/ChenYuheng-Apache": "Apache-2.0",
        "LICENSES/ChenYuheng-MIT": "MIT",
        "LICENSES/Ethiraric-Apache": "Apache-2.0",
        "LICENSES/Ethiraric-MIT": "MIT",
    },
    # zstd-sys compiles the bundled zstd library, which is offered under
    # BSD-3-Clause or GPL-2.0-only; the crate is relied on under BSD-3-Clause.
    "zstd-sys": {"zstd/COPYING": "GPL-2.0-only", "zstd/LICENSE": "BSD-3-Clause"},
}


class LicenseReview(ValueError):
    """A crate can only be used under a license that is not accepted."""


@dataclass(frozen=True)
class Notice:
    name: str
    version: str
    source: str | None
    declared: str
    relied: tuple[str, ...]
    texts: tuple[tuple[str, str], ...]
    fallbacks: tuple[str, ...]


def locked_metadata():
    """Resolve the locked graph with every feature of every workspace member."""
    output = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"],
        cwd=WORKSPACE,
    )
    return json.loads(output)


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


def read_text(path):
    """Read a notice verbatim, with LF line endings and a final newline."""
    try:
        text = path.read_bytes().decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError(f"{path}: notices must be UTF-8 text") from error
    text = text.replace("\r\n", "\n")
    return text if text.endswith("\n") else text + "\n"


def version_key(version):
    parts = re.split(r"[.+-]", version)
    return [(0, int(part)) if part.isdigit() else (1, part) for part in parts]


def dependency_closure(metadata, linked=False):
    """Return the package IDs that the shipped crates depend on, members included.

    The default follows normal and build edges for every target platform. The
    linked closure follows only normal edges and stops at procedural macros,
    which run inside the compiler instead of the shipped binaries.
    """
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [
        identity
        for identity in metadata["workspace_members"]
        if packages[identity]["name"] in SHIPPED
    ]
    if sorted(packages[identity]["name"] for identity in roots) != sorted(SHIPPED):
        raise ValueError("the workspace must contain each shipped crate once")
    seen, pending = set(), list(roots)
    while pending:
        identity = pending.pop()
        if identity in seen:
            continue
        seen.add(identity)
        for dependency in nodes[identity]["deps"]:
            kinds = {kind["kind"] for kind in dependency["dep_kinds"]}
            target = packages[dependency["pkg"]]
            if linked:
                follow = None in kinds and not any(
                    "proc-macro" in item["kind"] for item in target["targets"]
                )
            else:
                follow = bool(kinds - {"dev"})
            if follow:
                pending.append(dependency["pkg"])
    return seen


def third_party(metadata):
    """Return the third-party packages of the shipped closure in a stable order."""
    packages = {package["id"]: package for package in metadata["packages"]}
    members = set(metadata["workspace_members"])
    selected = [
        packages[identity]
        for identity in dependency_closure(metadata)
        if identity not in members
    ]
    return sorted(
        selected,
        key=lambda package: (package["name"], version_key(package["version"])),
    )


def crate_files(package):
    """Return the crate root and its notice files with the license each carries.

    The license is None for a general file, which is reproduced under any
    choice.
    """
    root = Path(package["manifest_path"]).parent
    label = f"{package['name']} {package['version']}"
    files = []
    for path in sorted(root.iterdir(), key=lambda path: path.name.lower()):
        if not LICENSE_LIKE.match(path.name) or not path.is_file():
            continue
        lowered = path.name.lower()
        if lowered in GENERAL_FILES:
            files.append((path.name, None))
        elif lowered in NAMED_FILES:
            files.append((path.name, NAMED_FILES[lowered]))
        else:
            raise ValueError(f"{label}: classify the license file {path.name}")
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


def crate_notice(package):
    """Select the texts reproduced for one crate."""
    label = f"{package['name']} {package['version']}"
    declared = package.get("license")
    if not declared:
        raise ValueError(f"{label}: declares no license expression; review it")
    root, files = crate_files(package)
    texts = {relative: read_text(root / relative) for relative, _ in files}
    shipped = frozenset().union(*(detect(text) for text in texts.values()))
    try:
        relied = choose(parse_expression(declared), shipped)
    except LicenseReview as error:
        raise ValueError(
            f"{label}: review the license {declared!r} ({error})"
        ) from error
    reproduced = []
    for relative, license in files:
        if license is None:
            reproduced.append((relative, texts[relative]))
        elif license in relied:
            if license not in detect(texts[relative]):
                raise ValueError(f"{label}: {relative} lacks the {license} text")
            reproduced.append((relative, texts[relative]))
    fallbacks = []
    for license in ordered(relied):
        if any(license in detect(text) for _, text in reproduced):
            continue
        if not (SPDX_TEXTS / f"{license}.txt").is_file():
            raise ValueError(f"{label}: ships no {license} text to reproduce")
        fallbacks.append(license)
    return Notice(
        name=package["name"],
        version=package["version"],
        source=source_label(package),
        declared=declared,
        relied=ordered(relied),
        texts=tuple(reproduced),
        fallbacks=tuple(fallbacks),
    )


def canonical_text(license):
    return read_text(SPDX_TEXTS / f"{license}.txt")


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


def render(notices):
    """Render the notices file from the selected crate texts."""
    # Shipped texts group by content; canonical texts group by license.
    blocks = {}

    def number(key, text, holder):
        block = blocks.setdefault(
            key, {"index": len(blocks) + 1, "text": text, "holders": []}
        )
        block["holders"].append(holder)
        return block["index"]

    entries = []
    for notice in notices:
        crate = f"{notice.name} {notice.version}"
        references = [
            f"[{number(text, text, f'{crate} ({relative})')}] {relative}"
            for relative, text in notice.texts
        ]
        for license in notice.fallbacks:
            index = number(("canonical", license), canonical_text(license), crate)
            references.append(
                f"[{index}] canonical {license} text; the crate package ships none"
            )
        entries.append(f"Crate: {crate}")
        if notice.source is not None:
            entries.append(f"    Source: {notice.source}")
        entries.append(f"    License: {notice.declared}")
        entries.append(f"    Relied on: {' AND '.join(notice.relied)}")
        entries += wrapped("Texts:", references, indent="    ")
        entries.append("")

    lines = ["yamaa third-party notices", "=" * 25, ""]
    lines += paragraph(
        "The yamaa Python wheels and the installed R package link third-party "
        "Rust crates into their native extension. This file lists those crates "
        "with the license, copyright and NOTICE texts that each crate ships. "
        f"The project's own license is in {PROJECT_LICENSE}; LICENSE-READSTAT.txt, "
        "LICENSE-UNICODE.txt and LICENSE-YAML.txt carry the notices for the "
        "vendored ReadStat sources, the Unicode data in yamaa's own tables and "
        "the vendored saphyr-parser crate."
    )
    lines += paragraph(
        "The list covers every third-party crate in the normal and build "
        "dependency closure of the shipped crates yamaa-python and yamaa-r, for "
        "every target platform, as locked in rust/Cargo.lock. Crates that run "
        "only while compiling, such as build scripts and procedural macros, and "
        "crates used only on other platforms are listed too. Crates come from "
        "crates.io unless an entry names another source."
    )
    lines += paragraph(
        "Where a crate offers a choice of licenses, yamaa relies on the first "
        "license in this order that the crate offers and whose text the crate "
        f"ships: {', '.join(ACCEPTED)}. Each entry names the license relied on. "
        "Texts that only grant an alternative license are not reproduced; the "
        "crate's general license, copyright, NOTICE and AUTHORS files are."
    )
    lines += paragraph(
        "Texts are reproduced verbatim, with LF line endings and a final "
        "newline. Identical texts are printed once and numbered; each entry "
        "names the numbered texts its crate ships. Where a crate package ships "
        "no text for the license it is relied on under, the entry says so and "
        f"the canonical text from the SPDX License List {SPDX_RELEASE} is "
        "reproduced instead, with its placeholders unfilled."
    )
    lines += paragraph(
        "This file is generated by tools/packaging/generate_third_party_notices.py "
        "from rust/Cargo.lock; do not edit it by hand."
    )
    lines += ["Crates", "-" * 6, ""] + entries
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
    return "\n".join(lines)


def notices_for(metadata):
    return [crate_notice(package) for package in third_party(metadata)]


def committed_entries(text):
    """Return the (name, version) pairs in a notices file's crate section."""
    crates = text.partition("\nCrates\n------\n")[2].partition("\nTexts\n-----\n")[0]
    return {
        (match.group(1), match.group(2))
        for match in re.finditer(r"(?m)^Crate: (\S+) (\S+)$", crates)
    }


def license_expression(metadata, notices):
    """Combine the members' licenses with those the linked crates rely on."""
    packages = {package["id"]: package for package in metadata["packages"]}
    members = set(metadata["workspace_members"])
    linked = dependency_closure(metadata, linked=True)
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
    """Require the package copies of the project license and notices to agree."""
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
        elif not r_copy.is_file() or r_copy.read_bytes() != python_copy.read_bytes():
            errors.append(f"R/yamaa/inst/{name}: must be identical to python/{name}")
    for directory, relative in ((PYTHON, "python"), (R_INST, "R/yamaa/inst")):
        for path in sorted(directory.glob("LICENSE*")):
            if path.name not in names:
                errors.append(
                    f"{relative}/{path.name}: not a package notice in license-files"
                )
    project_license = (ROOT / "LICENSE").read_bytes()
    for directory, relative in ((PYTHON, "python"), (R_INST, "R/yamaa/inst")):
        path = directory / PROJECT_LICENSE
        if path.is_file() and path.read_bytes() != project_license:
            errors.append(f"{relative}/{PROJECT_LICENSE}: must be a copy of LICENSE")
    return errors


def check(metadata):
    """Return every finding for the committed notices and license copies."""
    notices = notices_for(metadata)
    expected = render(notices).encode("utf-8")
    shipped = {(notice.name, notice.version) for notice in notices}
    errors = []
    for directory, relative in ((PYTHON, "python"), (R_INST, "R/yamaa/inst")):
        path = directory / NOTICES
        if not path.is_file():
            errors.append(f"{relative}/{NOTICES}: missing")
            continue
        committed = path.read_bytes()
        listed = committed_entries(committed.decode("utf-8"))
        for name, version in sorted(shipped - listed):
            errors.append(
                f"{relative}/{NOTICES}: no entry for {name} {version}, "
                "which the locked graph ships"
            )
        for name, version in sorted(listed - shipped):
            errors.append(
                f"{relative}/{NOTICES}: lists {name} {version}, "
                "which the locked graph no longer ships"
            )
        if committed != expected:
            errors.append(
                f"{relative}/{NOTICES}: differs from the notices generated from "
                "rust/Cargo.lock"
            )
    project = python_project()
    errors += check_copies(project)
    expression = license_expression(metadata, notices)
    if project["license"] != expression:
        errors.append(
            f"python/pyproject.toml: license must be {expression!r} to cover the "
            f"linked crates, not {project['license']!r}"
        )
    return notices, errors


def write(metadata):
    notices = notices_for(metadata)
    payload = render(notices).encode("utf-8")
    project_license = (ROOT / "LICENSE").read_bytes()
    for directory in (PYTHON, R_INST):
        (directory / NOTICES).write_bytes(payload)
        (directory / PROJECT_LICENSE).write_bytes(project_license)
    return notices


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="regenerate the notices and the project license copies",
    )
    args = parser.parse_args()
    try:
        metadata = locked_metadata()
        if args.write:
            notices = write(metadata)
            print(f"Wrote notices for {len(notices)} third-party crates")
            return
        notices, errors = check(metadata)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")
    if errors:
        parser.exit(
            1,
            "\n".join(errors)
            + "\nRun: python -m tools.packaging.generate_third_party_notices --write\n",
        )
    print(f"Third-party notices cover the {len(notices)} locked third-party crates")


if __name__ == "__main__":
    main()
