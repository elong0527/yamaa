"""Verify locked Cargo resolution and the engine bundles that source packages carry.

Without archives, it checks that the canonical rust/Cargo.lock is current and
that each host's bundle trims it to a lock that verifies crate by crate against
it, as `make check-repository` runs it, and that the R package's Makevars and
Cargo profile strip nothing. With archives, it checks the engine bundle (#3030
D55) of each Python source archive and R source package against the checkout,
that the archive holds no Rust toolchain file anywhere, and that an R source
package's Makevars and Cargo profile strip nothing. On Windows, R's own
toolchain links every package DLL with `-s`, a link flag of R's that the
package does not override, so the R package's Windows DLL carries no symbol
table (#3030 D140).
"""

import argparse
import dataclasses
import io
import json
import re
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path, PurePosixPath

from tools.paths import ROOT, WORKSPACE
from tools.staging import engine_bundle, vendor_crates


@dataclasses.dataclass(frozen=True)
class Layout:
    """Where a host's source archive keeps its bundle, below the archive root."""

    workspace: str
    config: str
    # The xz tar of the vendored crates, or None when they sit in vendor/.
    packed: str | None = None

    def owned(self):
        """The directories that hold only the bundle's files."""
        return (self.workspace, self.config.rpartition("/")[0] + "/")


LAYOUTS = {
    "python": Layout("rust/", ".cargo/config.toml"),
    # The Makevars unpack the crates and copy the configuration into the
    # build's Cargo home (tools/staging/stage_r_package.py).
    "r": Layout("src/rust/", "src/rust/vendor-config.toml", "src/rust/vendor.tar.xz"),
}
# The R package strips nothing itself: CRAN's Repository Policy asks a package
# not to remove diagnostic information such as symbols in shared objects. Its
# Makevars build with the release profile and override none of R's link flags,
# while the Python wheels strip their symbols with the dist profile
# (rust/Cargo.toml), which the R bundle's manifest leaves out
# (tools/staging/engine_bundle.py). The R package's shared object thus keeps its
# symbols on Linux and macOS. On Windows, R's own toolchain links every package
# DLL with -s, so the DLL carries no symbol table there (#3030 D140).
R_PACKAGE = ROOT / "R/yamaa"
R_MAKEVARS = ("src/Makevars.in", "src/Makevars.win.in")
STRIPS_NOTHING = (
    "the R package strips nothing itself, as CRAN's Repository Policy asks "
    "(on Windows, R's own link flags strip every package DLL)"
)
# R's linker, not rustc, links the engine's static library into the shared
# object, so Cargo's strip never reaches it: R's link step is the one that
# could strip it. These linker options remove symbols or debug information,
# as the compiler driver passes them on after -Wl, or -Xlinker: -s
# (--strip-all), -S (--strip-debug) and -x (--discard-all) for GNU ld and lld,
# and -S and -x for macOS's ld64.
LINKER_STRIP = {"s", "S", "x", "discard-all"}
# make's $(strip ...) function trims whitespace and strips nothing.
MAKE_STRIP = re.compile(r"\$[({]strip\s")
# make's conditional directives, and the line that opens a define body.
CONDITIONAL = re.compile(r"(?:ifeq|ifneq|ifdef|ifndef|else|endif)\b")
DEFINE = re.compile(r"(?:(?:override|export|private)\s+)*define\b")
# A variable assignment, with its modifiers, or another directive: a make line
# that opens no rule.
ASSIGNMENT = re.compile(
    r"(?:(?:override|export|unexport|private)\s+)*"
    r"(?:[^\s:#=]+\s*(?::{1,3}=|[+?!]?=)"
    r"|(?:undefine|vpath|-?include|sinclude|-?load)\b)"
)
# A # that no backslash escapes starts a make comment.
COMMENT = re.compile(r"(?<!\\)#")
# Recipe commands that strip without the word strip: objcopy's -g
# (--strip-debug), -S (--strip-all) and -x (--discard-all), and install -s.
TOOL_STRIP = {"objcopy": {"-g", "-S", "-x", "--discard-all"}, "install": {"-s"}}
# A shell command ends at ;, &&, ||, | or a single &, but not at the & of a
# redirection such as 2>&1 or &>log. The quotes are gone by then, so a quoted
# separator, such as the ; in the Windows cargo line's LIBRARY_PATH, ends one
# too.
COMMAND_END = re.compile(r"&&|\|\||[;|]|(?<![<>])&(?!>)")
# A word that can come before a shell command's name, with or without make's
# @, - or + recipe prefix: an assignment such as FOO=1, a shell keyword that a
# command follows, or one of the launchers env, command, exec, nice, nohup and
# time, which run the words after them as a command. A launcher's own options,
# as in env -i, are not read.
BEFORE_NAME = re.compile(
    r"[@+-]*(?:[A-Za-z_][A-Za-z0-9_]*=.*"
    r"|[!{(]|if|then|elif|else|while|until|do"
    r"|env|command|exec|nice|nohup|time)"
)


def makevars_profile(label, makevars):
    """The Cargo profile that a Makevars template's one cargo build selects.

    Refuses a command that sets strip itself, as a variable, flag or
    configuration value.
    """
    commands = [
        line
        for line in makevars.replace("\r\n", "\n").replace("\\\n", " ").splitlines()
        if '"$(CARGO)" build ' in line
    ]
    if len(commands) != 1:
        raise ValueError(f"{label}: expected one cargo build command")
    if "strip" in commands[0].lower():
        raise ValueError(f"{label}: the cargo build command sets strip")
    words = commands[0].split()
    profiles = []
    for index, word in enumerate(words):
        if word in ("--release", "-r"):
            profiles.append("release")
        elif word == "--profile" and index + 1 < len(words):
            profiles.append(words[index + 1])
        elif word.startswith("--profile="):
            profiles.append(word.removeprefix("--profile="))
    if len(profiles) > 1:
        raise ValueError(f"{label}: the cargo build selects more than one profile")
    return profiles[0] if profiles else "dev"


def logical_lines(makevars):
    """make's logical lines, each as its physical lines: a line that ends in an
    odd number of backslashes continues on the next, and make reads CRLF as
    LF."""
    lines = [[]]
    for line in makevars.replace("\r\n", "\n").split("\n"):
        lines[-1].append(line)
        if (len(line) - len(line.rstrip("\\"))) % 2 == 0:
            lines.append([])
    return [physical for physical in lines if physical]


def opens_rule(text):
    """Whether a make line, neither a recipe nor a conditional, opens a rule:
    a target line, as opposed to an assignment, a directive or a
    target-specific variable.

    A trailing comment is cut first, so that a colon in it, as in a URL, does
    not make a directive look like a rule. make reads a # inside a variable
    reference literally; cutting there too can only close a rule early, which
    gives the lines after it the stricter test.
    """
    text = COMMENT.split(text, maxsplit=1)[0]
    if ASSIGNMENT.match(text):
        return False
    depth = 0
    for index, char in enumerate(text):
        depth += (char in "({") - (char in ")}")
        if char == ":" and depth <= 0:
            rest = text[index:].lstrip(":")
            if not rest.startswith("="):
                return not ASSIGNMENT.match(rest.lstrip())
    return False


def strips_at_link(previous, word, recipe):
    """Whether a word, after `previous`, passes a strip option to a link.

    The word's comma-separated pieces count as words, as in make's
    $(if $(DEBUG),,-s), and make's parentheses and braces around them hide
    nothing. The pieces after -Wl, and a word after -Xlinker, are linker
    options. The compiler driver's -s counts outside recipes only: a recipe's
    -s belongs to its own command, as in ln -s.
    """
    linker = previous.split(",")[-1].strip("(){}") == "-Xlinker"
    for part in word.split(","):
        piece = part.strip("(){}")
        if piece == "-Wl":
            linker = True
        elif piece == "-s" and not recipe:
            return True
        elif linker and piece.startswith("-") and piece.lstrip("-") in LINKER_STRIP:
            return True
    return False


def refuse_strip(label, line, recipe):
    """Refuse a make line that mentions strip or passes a strip option, or a
    recipe command in TOOL_STRIP that strips."""
    text = MAKE_STRIP.sub(" ", line.replace('"', "").replace("'", ""))
    words = text.split()
    for previous, word in zip(["", *words], words):
        if "strip" in word.lower():
            shown = word
        elif strips_at_link(previous, word, recipe):
            shown = word.strip("(){}")
        else:
            continue
        problem = (
            f"a recipe runs {shown}"
            if recipe
            else f"{shown} strips the shared object at R's link step"
        )
        raise ValueError(f"{label}: {problem}; {STRIPS_NOTHING}")
    if not recipe:
        return
    # Each shell command is named by its first word, as in @objcopy or
    # $(OBJCOPY), or a cross tool's x86_64-w64-mingw32-objcopy, once the words
    # that BEFORE_NAME matches are skipped, as in env FOO=1 objcopy or
    # then install.
    for command in COMMAND_END.split(text):
        words = command.split()
        while words and BEFORE_NAME.fullmatch(words[0]):
            del words[0]
        tool, *options = words or [""]
        name = tool.strip("@+-$(){}").rsplit("/", 1)[-1].lower()
        for key, strips in TOOL_STRIP.items():
            if name.endswith(key) and (found := strips.intersection(options)):
                raise ValueError(
                    f"{label}: a recipe runs {tool} {min(found)}; {STRIPS_NOTHING}"
                )


def check_r_link(label, makevars):
    """Refuse a Makevars template that would strip the shared object at R's
    link step.

    Outside comments, nothing may mention strip: a command such as strip or
    $(STRIP_SHARED_LIB) in a recipe, or an option such as -Wl,--strip-all. No
    line may pass a linker option in LINKER_STRIP after -Wl, or -Xlinker.
    Outside the recipes, in PKG_LIBS or a variable that it reads, no line may
    pass the compiler driver's -s either, and no recipe may run a command in
    TOOL_STRIP with its strip option, also behind a launcher, an assignment or
    a shell keyword. make's $(strip ...) function is admitted, but not an
    option inside it.

    The lines are read as make reads them. A tab-led line is a recipe only
    while a rule is open: from a target line, through blank and comment
    lines, to the next other line. A define body is a variable's value,
    whatever its lines look like. make keeps a rule open through a
    conditional, but this check, which evaluates none, closes it there, since
    a rule that one branch opens is not open in the other.
    """
    rule, depth = False, 0
    for physical in logical_lines(makevars):
        line = " ".join(part.removesuffix("\\") for part in physical)
        text = line.strip()
        if depth:
            # make counts define and endef on lines that do not start with a tab.
            if not line.startswith("\t"):
                depth += bool(re.match(r"define\b", text))
                depth -= bool(re.match(r"endef\b", text))
            if depth:
                refuse_strip(label, line, recipe=False)
        elif rule and line.startswith("\t"):
            # make hands a recipe line to the shell with its backslash-newlines
            # and drops the tab that leads each continuation line. The shell
            # joins the lines there without a space, but a comment ends at its
            # newline, so it does not hide the line after it.
            commands, command = [], ""
            for part in physical:
                if part.lstrip().startswith("#"):
                    commands.append(command)
                    command = ""
                else:
                    piece = part.removeprefix("\t") if command else part
                    command += piece.removesuffix("\\")
            for shell_command in [*commands, command]:
                refuse_strip(label, shell_command, recipe=True)
        elif text and not text.startswith("#"):
            refuse_strip(label, line, recipe=False)
            if DEFINE.match(text):
                rule, depth = False, 1
            else:
                rule = not CONDITIONAL.match(text) and opens_rule(text)


def check_r_symbols(label, manifest, config, makevars):
    """Refuse an R build that would strip its shared object.

    `manifest` and `config` are the texts of the bundle's workspace manifest
    and Cargo configuration, and `makevars` maps each Makevars template's name
    to its text. Neither the profile that a template's cargo build selects nor
    a profile that it inherits from may set strip, other than to false or
    "none", in the manifest or in the configuration, and no template may
    strip at R's link step (check_r_link).
    """
    sources = [
        ("the Cargo configuration", tomllib.loads(config).get("profile", {})),
        ("the workspace manifest", tomllib.loads(manifest).get("profile", {})),
    ]
    for name, text in makevars.items():
        selected = makevars_profile(f"{label}: {name}", text)
        check_r_link(f"{label}: {name}", text)
        profile, seen = selected, set()
        while profile is not None and profile not in seen:
            seen.add(profile)
            for where, profiles in sources:
                if profiles.get(profile, {}).get("strip", False) not in (False, "none"):
                    use = (
                        f"which {name} builds with"
                        if profile == selected
                        else f"which the {selected} profile of {name} inherits"
                    )
                    raise ValueError(
                        f"{label}: {where} sets strip in the {profile} profile, "
                        f"{use}; {STRIPS_NOTHING}"
                    )
            profile = next(
                (
                    profiles[profile]["inherits"]
                    for _, profiles in sources
                    if "inherits" in profiles.get(profile, {})
                ),
                None,
            )


def locked_metadata(workspace: Path) -> str:
    if not (workspace / "Cargo.lock").is_file():
        raise ValueError("canonical Cargo.lock is missing")
    return subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=workspace,
        text=True,
    )


def bundle_files(host, workspace=WORKSPACE):
    """The bytes of each file of a host's bundle that comes from the checkout,
    by its path in the source archive."""
    layout = LAYOUTS[host]
    files = {
        layout.workspace + relative: engine_bundle.source_bytes(source, workspace)
        for relative, source in engine_bundle.sources(host, workspace)
    }
    files[layout.workspace + "Cargo.toml"] = engine_bundle.workspace_manifest(
        host, workspace
    ).encode()
    files[layout.config] = engine_bundle.CARGO_CONFIG.encode()
    return files


def refuse_unsafe(label, entries):
    """Refuse duplicate, linked, absolute or ambiguous archive members."""
    if len({entry.name for entry in entries}) != len(entries):
        raise ValueError(f"{label}: duplicate archive member")
    if any(not (entry.isfile() or entry.isdir()) for entry in entries):
        raise ValueError(f"{label}: archive must not contain links")
    if any(
        "\\" in entry.name
        or any(part in {"", ".", ".."} for part in entry.name.rstrip("/").split("/"))
        for entry in entries
    ):
        raise ValueError(f"{label}: archive has an invalid member name")


def toolchain_files(names):
    """The archive member names that are Rust toolchain files."""
    return sorted(
        name
        for name in names
        if PurePosixPath(name).name in vendor_crates.TOOLCHAIN_FILES
    )


def packed_crates(label, data):
    """Read an xz tar of vendored crates in one pass.

    Refuses unsafe members, members outside vendor/ and Rust toolchain files.
    Returns the size of each of its files, by name below vendor/, and the
    parsed `.cargo-checksum.json` of each crate, by crate directory.
    """
    entries, records = [], {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r|xz") as packed:
        for entry in packed:
            entries.append(entry)
            parts = entry.name.split("/")
            if entry.isfile() and parts[2:] == [vendor_crates.CHECKSUMS]:
                records[parts[1]] = json.loads(packed.extractfile(entry).read())
    refuse_unsafe(label, entries)
    top = vendor_crates.DIRECTORY
    if any(entry.name.split("/")[0] != top for entry in entries):
        raise ValueError(f"{label}: archive members must sit below {top}/")
    for name in toolchain_files(entry.name for entry in entries):
        raise ValueError(f"{label} carries {name}")
    files = {
        entry.name.removeprefix(f"{top}/"): entry.size
        for entry in entries
        if entry.isfile()
    }
    return files, records


def check_vendored(label, files, records, kept):
    """Check vendored crates against the lock that selects them.

    `files` maps the vendored file names below vendor/ to their sizes. Every
    crates.io package of the lock must have one crate, holding exactly the
    files that its `.cargo-checksum.json` lists. A crate whose record lists
    its manifest alone, as for a crate that no supported target compiles,
    may also hold the empty library root that Cargo needs to read that
    manifest (tools/staging/vendor_crates.py, stub).
    """
    crates = {}
    for name in files:
        crate, _, relative = name.partition("/")
        crates.setdefault(crate, set()).add(relative)
    registry = {f"{name}-{version}" for name, version, source in kept if source}
    for crate in sorted(set(crates) ^ registry):
        raise ValueError(
            f"{label}: vendored crate {crate} differs from the lock's crates"
        )
    for crate, names in sorted(crates.items()):
        if crate not in records:
            raise ValueError(
                f"{label}: vendored crate {crate} has no {vendor_crates.CHECKSUMS}"
            )
        listed = set(records[crate]["files"])
        library = f"{crate}/{vendor_crates.LIBRARY}"
        if listed == {vendor_crates.MANIFEST} and files.get(library) == 0:
            listed.add(vendor_crates.LIBRARY)
        if names - {vendor_crates.CHECKSUMS} != listed:
            raise ValueError(
                f"{label}: vendored crate {crate} differs from the files "
                "its .cargo-checksum.json lists"
            )


def check_bundle_archive(path: Path, workspace=WORKSPACE):
    """Check a source archive's engine bundle against the checkout.

    The archive must hold no Rust toolchain file anywhere, the vendored crates
    included. It must carry exactly the bundle's Rust sources, the selftest
    cases that the facade embeds, the copies of the schema files that the
    adapters embed, its manifest and Cargo configuration, a lock that
    verifies against the canonical lock, and each crates.io crate
    of that lock, with every file that its `.cargo-checksum.json` lists and
    no other, apart from the empty library root of a crate that keeps only
    its manifest: in vendor/ in a Python source archive, and in
    src/rust/vendor.tar.xz in an R source package. An R source
    package's Makevars must strip nothing, through the Cargo profile or at R's
    link step (check_r_symbols). Returns the host and the lock's packages.
    """
    canonical = (workspace / "Cargo.lock").read_bytes().decode()
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        refuse_unsafe(path.name, entries)
        roots = {entry.name.split("/")[0] for entry in entries}
        if len(roots) != 1:
            raise ValueError(f"{path.name}: source archive must have one root")
        root = roots.pop()
        for name in toolchain_files(
            entry.name.removeprefix(f"{root}/") for entry in entries
        ):
            raise ValueError(f"{path.name}: source archive carries {name}")
        files = {
            entry.name.removeprefix(f"{root}/"): entry
            for entry in entries
            if entry.isfile()
        }

        def read(name):
            if name not in files:
                raise ValueError(f"{path.name}: the bundle is missing {name}")
            return archive.extractfile(files[name]).read()

        host = "r" if LAYOUTS["r"].workspace + "Cargo.lock" in files else "python"
        layout = LAYOUTS[host]
        expected = bundle_files(host, workspace)
        lock = layout.workspace + "Cargo.lock"
        carried = {
            name
            for name in files
            if name.startswith(layout.owned()) and name != layout.packed
        }
        for name in sorted(carried ^ (set(expected) | {lock})):
            raise ValueError(
                f"{path.name}: the bundle differs from the checkout at {name}"
            )
        for name, payload in sorted(expected.items()):
            if read(name) != payload:
                raise ValueError(f"{path.name}: {name} differs from the checkout")
        if host == "r":
            templates = {}
            for name in R_MAKEVARS:
                if name not in files:
                    raise ValueError(f"{path.name}: the R package is missing {name}")
                templates[name] = archive.extractfile(files[name]).read().decode()
            check_r_symbols(
                path.name,
                read(layout.workspace + "Cargo.toml").decode(),
                read(layout.config).decode(),
                templates,
            )
        kept = engine_bundle.verify_lock(
            read(lock).decode(), canonical, engine_bundle.members(host)
        )
        if layout.packed is None:
            label, top = path.name, vendor_crates.DIRECTORY + "/"
            vendored = {
                name.removeprefix(top): entry.size
                for name, entry in files.items()
                if name.startswith(top)
            }
            records = {
                name.partition("/")[0]: json.loads(read(top + name))
                for name in vendored
                if name.partition("/")[2] == vendor_crates.CHECKSUMS
            }
        else:
            label = f"{path.name}: {layout.packed}"
            vendored, records = packed_crates(label, read(layout.packed))
    check_vendored(label, vendored, records, kept)
    return host, kept


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="*", type=Path)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    metadata = locked_metadata(WORKSPACE)
    for archive in args.archives:
        host, kept = check_bundle_archive(archive)
        print(
            f"{archive.name}: {host} engine bundle with {len(kept)} canonical lock "
            "packages and their vendored crates, and no Rust toolchain file"
        )
    if not args.archives:
        with tempfile.TemporaryDirectory(prefix="yamaa-bundle-lock-") as scratch:
            for host in sorted(engine_bundle.BINDINGS):
                kept = engine_bundle.stage_workspace(host, Path(scratch) / host)
                print(
                    f"{host} bundle: the trimmed lock keeps {len(kept)} canonical "
                    "packages, the dependency closure of "
                    + ", ".join(engine_bundle.members(host))
                )
        check_r_symbols(
            "R/yamaa",
            engine_bundle.workspace_manifest("r"),
            engine_bundle.CARGO_CONFIG,
            {
                name: (R_PACKAGE / name).read_text(encoding="utf-8")
                for name in R_MAKEVARS
            },
        )
        print(
            "R package: its Makevars strip nothing, through the Cargo profile "
            "or at R's link step"
        )
    if args.output_dir:
        args.output_dir.mkdir(parents=True, exist_ok=True)
        (args.output_dir / "cargo-metadata.json").write_text(metadata)
        for name, command in (
            ("cargo-version.txt", ["cargo", "--version"]),
            ("rustc-version.txt", ["rustc", "-vV"]),
            (
                "cargo-features.txt",
                [
                    "cargo",
                    "tree",
                    "--locked",
                    "--workspace",
                    "--target",
                    "all",
                    "-e",
                    "features",
                ],
            ),
        ):
            output = subprocess.check_output(command, cwd=WORKSPACE, text=True)
            (args.output_dir / name).write_text(output)
    print(f"Locked Cargo graph: {len(json.loads(metadata)['packages'])} packages")


if __name__ == "__main__":
    main()
