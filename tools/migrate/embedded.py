"""YAML embedded in Rust, Python and R test sources and in JSON and TSV fixtures.

`python -m tools.migrate --embedded SOURCE... --output DIR` reads every
string a source spells with literals (`tools.migrate.sources`) and every
string and specification object of a JSON or TSV fixture
(`tools.migrate.fixtures`), converts the YAML among them, rewrites the spec
paths around it, and writes each changed source under the target directory,
never in place. P2-9 and P2-10 of #3030 run it on the Rust tests and the host
suites.

It adds no transform of its own. A document is converted by the file
converter (`tools.migrate.convert`) as a loose file of the name the source
writes it to, `document.yaml` when no call names one, so it gives the same
YAML values here as in a file. A fixture case, a JSON object naming its parent
layers and inputs by file name, converts its documents together with those
files, so keys and clears read them. A fragment is converted as a Markdown
block is, and only when it shows a 0.5 spelling. JSON converts through its
block rendering and is written back in its layout (`tools.migrate.fixtures`).
A spec-path string is rewritten through the rename table's `paths` block
(P-CLR-2), against the specification beside it: one in the same function,
test case or fixture row, or bound to a name used there.

What it cannot convert it lists for manual work instead:

- a string built by interpolation whose YAML the conversion would change,
  read with a plain word in each hole; the converter never guesses a hole;
- a pattern or probe (`replace`, `contains`, `startswith` and their kin)
  whose YAML would change, since it would no longer match;
- a 0.5 document that does not parse as written, such as a prefix code
  completes, or whose conversion would not parse;
- JSON the conversion changes, in a layout `json.dumps` does not write;
- a spec path naming an intermediate or row template by index where no
  specification beside it, or more than one, settles the member, and a
  changed spec path inside a longer string (a report row, a template that
  builds the path);
- a string the file converter fails on, left as written.

The file converter's notes carry over at the source line they concern, except
those an embedded document always draws: `model` (P-MODEL-1 leaves it
optional for test specs) and `entry` (a project's specifications are other
strings of the test).
"""

import os
import re
import tempfile
from pathlib import Path

import tools.migrate as migrate
import yaml
from tools.migrate import contracts, fixtures, markdown, sources
from tools.migrate import table as table_module
from tools.migrate.convert import Note
from tools.migrate.text import compose, value_of
from tools.migrate.unit import (
    COMMENT_SPELLINGS,
    Result,
    check_table,
    classify,
    relative,
)

READERS = {
    ".rs": sources.rust_extraction,
    ".py": sources.python_extraction,
    ".r": sources.r_extraction,
    ".json": fixtures.json_extraction,
    ".tsv": fixtures.tsv_extraction,
}
SKIPPED = frozenset({"target", "__pycache__", "node_modules", "vendor"})
SILENT = frozenset({"model", "entry"})
DOCUMENT = "document.yaml"
HEADER = re.compile(r"schema_version\s*:")
PATH = re.compile(r"[A-Za-z_][\w-]*(?:\.[\w-]+|\[\d+\]|\['(?:[^']|'')*'\])+")
# A member addressed by an index a template fills in, such as rows[{i}].
TEMPLATED_INDEX = re.compile(
    r"(?<![\w.])(?:intermediates|rows)\[(?:\{[^{}]*\}|%s)\]" % sources.HOLE_WORD
)
ROOTS = frozenset(
    "parent domain keys input base filter output windows intermediates columns"
    " rows verifications submission metadata environments codelists study sdtm"
    " adam send".split()
)


def files_under(path):
    """The files under `path` the embedded converter reads."""
    if path.is_file():
        return [path]
    return sorted(
        item
        for item in path.rglob("*")
        if item.is_file()
        and item.suffix.lower() in READERS
        and not any(
            part.startswith(".") or part in SKIPPED
            for part in item.relative_to(path).parts[:-1]
        )
    )


def convert_embedded(paths, base=None, table=None):
    """Convert the YAML embedded in `paths`; the Result holds each changed file."""
    table = table or table_module.load()
    check_table(table)
    paths = [Path(path).resolve() for path in paths]
    for path in paths:
        if not path.exists():
            raise FileNotFoundError(f"{path} does not exist")
    if base is None:
        anchors = [path if path.is_dir() else path.parent for path in paths]
        base = Path(os.path.commonpath(anchors)) if anchors else Path.cwd()
    base = Path(base).resolve()
    result = Result()
    for path in paths:
        for file in files_under(path):
            label = str(relative(file, base))
            text = file.read_text(encoding="utf-8")
            reader = READERS[file.suffix.lower()]
            converted = Source(label, text, table, result, reader).run()
            result.stats["source files read"] += 1
            if converted != text:
                result.files[label] = converted
                result.stats["source files changed"] += 1
    return result


def convert_files(documents, table, inputs=None):
    """The file converter's output for documents written at their names, beside
    the input files given: {name: text, or None when copied unchanged}, and the
    conversion's Result."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for name, text in {**(inputs or {}), **documents}.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8", newline="")
        result = migrate.convert(
            [root / name for name in documents], base=root, table=table
        )
    outputs = {}
    for name in documents:
        output = result.files.get(name)
        outputs[name] = output if isinstance(output, str) else None
    return outputs, result


def file_name(hint):
    """The name a document converts under: the name it is written to, kept
    relative, else `document.yaml`."""
    parts = [
        part
        for part in (hint or "").replace("\\", "/").split("/")
        if part not in ("", ".")
    ]
    if not parts:
        return DOCUMENT
    if hint.startswith("/") or ".." in parts:
        return parts[-1]
    return "/".join(parts)


def parse(text):
    """The node tree of a mapping or a sequence whose keys are all scalars, or None."""
    try:
        root = compose(text)
    except yaml.YAMLError:
        return None
    if not isinstance(root, (yaml.MappingNode, yaml.SequenceNode)):
        return None
    stack = [root]
    while stack:
        node = stack.pop()
        if isinstance(node, yaml.MappingNode):
            for key, value in node.value:
                if not isinstance(key, yaml.ScalarNode):
                    return None
                stack.append(value)
        elif isinstance(node, yaml.SequenceNode):
            stack.extend(node.value)
    return root


def composes(text):
    try:
        compose(text)
    except yaml.YAMLError:
        return False
    return True


def names_of(root):
    if isinstance(root, yaml.MappingNode):
        return {key.value for key, _ in root.value}
    return set()


def changes(text, table):
    """Whether the file converter would change a text, as a document or a fragment."""
    root = parse(text)
    if root is None:
        try:
            compose(text)
        except yaml.YAMLError:
            return bool(COMMENT_SPELLINGS.search(text))
        return "schema_version" in text
    names = names_of(root)
    if "schema_version" in names:
        return True
    if "yamaa" in names:
        return False
    converted, _ = markdown.convert_block(text, None, table, [], "", 0)
    return converted is not None and converted != text


def yaml_candidate(text):
    """True for text that may be YAML worth reading: more than one line, flow
    style, or a header line; never a word or a sentence."""
    stripped = text.strip()
    return bool(stripped) and (
        "\n" in stripped or stripped[0] in "{[" or HEADER.match(stripped) is not None
    )


def first_line(text):
    for line in text.splitlines():
        if line.strip():
            return line.strip()[:100]
    return ""


class Source:
    """One source file: its groups converted, its paths rewritten, its notes."""

    def __init__(self, label, text, table, result, reader):
        self.label = label
        self.text = text
        self.table = table
        self.result = result
        self.reader = reader
        self.extraction = reader(text)
        self.edits = []
        self.pending = []
        self.specs = []  # (scopes, 0.5 spec value) of each specification read
        self.surfaces = []  # (scope, language version) of each document read

    def note(self, offset, kind, message):
        """A manual follow-up, located once the file's edits are known."""
        self.pending.append((offset, kind, message))

    def count(self, name, amount=1):
        self.result.stats[name] += amount

    def changes(self, text):
        """`changes`, counting a text the file converter fails on as changing,
        so it is listed rather than missed."""
        try:
            return changes(text, self.table)
        except Exception:  # one odd string must not stop a batch run
            return True

    def run(self):
        work, paths = [], []
        for group in self.extraction.groups:
            kind = self.classify(group)
            if kind == "path":
                paths.append(group)
            elif kind is not None:
                work.append((group, kind))
        if self.reader is fixtures.json_extraction:
            together = self.convert_cases(
                [group for group, kind in work if kind == "document"]
            )
            work = [item for item in work if item[0] not in together]
        for group, kind in work:
            self.convert(group, kind)
        self.mixed()
        for group in paths:
            self.rewrite_path(group)
        for group in self.extraction.groups:
            if group.converted is None and group not in paths:
                self.path_inside(group)
        return self.finish()

    # -- reading ---------------------------------------------------------------

    def classify(self, group):
        """document, fragment, path or None, listing what needs manual work."""
        if group.interpolated:
            self.count("interpolated strings read")
            if self.changes(group.skeleton):
                self.count("interpolated strings listed")
                self.note(
                    group.start,
                    "interpolated",
                    "a string built by interpolation holds YAML the conversion"
                    f" changes; convert it by hand: {first_line(group.template)}",
                )
            return None
        text = group.text
        if PATH.fullmatch(text) and re.split(r"[.\[]", text, maxsplit=1)[0] in ROOTS:
            return "path"
        if group.role != "value":
            if HEADER.search(text) or (yaml_candidate(text) and self.changes(text)):
                self.count("patterns listed")
                self.note(
                    group.start,
                    "pattern",
                    f"a {group.role} argument holds YAML the conversion changes, so it"
                    " would no longer match the converted text; rewrite it by hand:"
                    f" {first_line(text)}",
                )
            return None
        if not yaml_candidate(text):
            return None
        root = parse(text)
        if root is None:
            if "schema_version" in text and self.changes(text):
                self.count("unparsed documents listed")
                self.note(
                    group.start,
                    "unparsed",
                    "a 0.5 document that does not parse as written, such as a prefix"
                    f" code completes; convert it by hand: {first_line(text)}",
                )
            return None
        names = names_of(root)
        if "yamaa" in names and "schema_version" not in names:
            self.surfaces.append((group.scope, "1.0"))
            return None
        if "schema_version" in names:
            self.surfaces.append((group.scope, "0.5"))
            if classify(Path(file_name(group.name)), root) == "spec":
                self.specs.append(((group.scope, *group.uses), value_of(root)))
        return "document" if "schema_version" in names else "fragment"

    # -- converting --------------------------------------------------------------

    def convert(self, group, kind):
        """Convert one document or fragment alone."""
        text, notes = self.readable(group), []
        try:
            if kind == "fragment":
                converted, _ = markdown.convert_block(
                    text, None, self.table, notes, "", 0
                )
            else:
                name = file_name(group.name)
                outputs, result = convert_files({name: text}, self.table)
                converted, notes = outputs[name], result.notes
        except Exception as error:  # one odd string must not stop a batch run
            self.count("strings the file converter failed on")
            self.note(
                group.start,
                "internal",
                f"the file converter failed on this string ({type(error).__name__}:"
                f" {error}); it is left as written",
            )
            return
        self.settle(group, kind, text, converted, notes)

    def convert_cases(self, documents):
        """Convert each fixture case's documents together with the files the case
        names (parent layers, inputs), so keys and clears read them; return the
        documents converted."""
        named = [
            group
            for group in self.extraction.groups
            if group.name and not group.interpolated and group.role == "value"
        ]
        done = set()
        for case in sorted({group.scope for group in documents if not group.name}):
            inside = [group for group in named if group.scope[: len(case)] == case]
            members = [
                group
                for group in documents
                if (group.scope == case and not group.name) or group in inside
            ]
            inputs = {
                file_name(group.name): group.text
                for group in inside
                if group.name.endswith(".csv")
            }
            names, clash = {}, False
            for group in members:
                name = file_name(group.name or f"{group.label or 'document'}.yaml")
                if classify(Path(name), parse(self.readable(group))) == "environment":
                    continue  # converted alone: a test source gains no function file (D8)
                clash = clash or name in names
                names[name] = group
            chosen = set(names.values())
            if clash or done & chosen or (len(names) < 2 and not inputs):
                continue
            texts = {name: self.readable(group) for name, group in names.items()}
            try:
                outputs, result = convert_files(texts, self.table, inputs)
            except Exception:  # each document converts alone instead
                continue
            for name, group in names.items():
                notes = [note for note in result.notes if note.file == name]
                self.count("documents converted with their case's files")
                self.settle(group, "document", texts[name], outputs[name], notes)
            done |= chosen
        return done

    def readable(self, group):
        """The YAML text the file converter reads for a group: JSON as its block rendering."""
        if fixtures.is_json(group.text):
            return fixtures.rendered(group.text)
        return group.text

    def settle(self, group, kind, text, converted, notes):
        """Carry a conversion's notes over and splice it in, unless it does not
        parse or its JSON layout cannot be written back."""
        located = text == group.text
        self.count("documents read" if kind == "document" else "fragments read")
        for note in notes:
            if note.kind in SILENT:
                self.count(f"{note.kind} notes left out")
            elif located:
                self.carry(group, note)
            else:
                self.note(group.start, note.kind, note.message)
        if (
            kind == "document"
            and names_of(parse(text)) == {"schema_version"}
            and not group.name
        ):
            self.note(
                group.start,
                "header",
                "a bare header with no file name beside it converts as a spec;"
                " give it the type of the file it stands for",
            )
        if converted is None or converted == text:
            return
        if not composes(converted):
            self.count("conversions that do not parse")
            self.note(
                group.start,
                "unparsed",
                "its conversion does not parse, so it is left as written;"
                f" convert it by hand: {first_line(group.text)}",
            )
            return
        if not located:
            converted, reason = fixtures.written_back(group.text, group.base, converted)
            if reason:
                self.count("JSON texts listed")
                self.note(
                    group.start,
                    "json",
                    f"JSON the conversion changes, {reason}; convert it by hand",
                )
                return
            if converted == group.text:
                return
        self.count(
            "documents converted" if kind == "document" else "fragments converted"
        )
        self.edits.extend(group.rewrite(converted))
        group.converted = converted

    def carry(self, group, note):
        """A file-converter note at the source line of the document line it names."""
        offset = group.start
        if note.line:
            lines = group.text.split("\n")
            index = sum(len(line) + 1 for line in lines[: note.line - 1])
            offset = group.offset_of(min(index, max(len(group.text) - 1, 0)))
        self.note(offset, note.kind, note.message)

    def mixed(self):
        """Where a scope writes both surfaces, the test may compare them (D115)."""
        newer = {scope for scope, version in self.surfaces if version == "1.0"}
        for group in self.extraction.groups:
            if group.converted is not None and group.scope in newer:
                self.note(
                    group.start,
                    "surfaces",
                    "this scope also writes a 1.0 document, so it may test both"
                    " surfaces (D115); check that converting this one keeps its meaning",
                )

    # -- spec paths ----------------------------------------------------------------

    def rewrite_path(self, group):
        text = group.text
        new = contracts.rewrite_path(text, {}, [], self.table)
        if new is None:
            new = self.resolve(group)
        if new is None or new == text:
            return
        self.count("spec paths rewritten")
        self.edits.extend(group.rewrite(new))
        group.converted = new

    def path_inside(self, group):
        """List a spec path with a changed segment inside a longer string, such as
        a printed report row or a template that builds the path (P-CLR-2)."""
        text = group.template if group.interpolated else group.text
        found = TEMPLATED_INDEX.search(text)
        found = found.group() if found else None
        for match in PATH.finditer(text) if found is None else ():
            path = match.group()
            if re.split(r"[.\[]", path, maxsplit=1)[0] not in ROOTS:
                continue
            if contracts.rewrite_path(path, {}, [], self.table) != path:
                found = path
                break
        if found is not None:
            self.count("strings holding a spec path listed")
            self.note(
                group.start,
                "path",
                f"a string holds the spec path {found}, which P-CLR-2 or the rename"
                " table changes; rewrite it by hand",
            )

    def resolve(self, group):
        """A path rewritten against the specifications nearest to it, or None."""
        scope = group.scope
        candidates = []
        for depth in range(len(scope), -1, -1):
            prefix = scope[:depth]
            candidates = [
                value
                for scopes, value in self.specs
                if any(where[:depth] == prefix for where in scopes)
            ]
            if candidates:
                break
        found = {
            contracts.rewrite_path(group.text, value, [], self.table)
            for value in candidates
        }
        found.discard(None)
        if len(found) == 1:
            return found.pop()
        self.count("spec paths listed")
        reason = (
            "the specifications beside it name different members"
            if found
            else "no specification beside it names that member"
        )
        self.note(
            group.start,
            "path",
            f"spec path {group.text} names a member by index, and {reason};"
            " rewrite it by hand (P-CLR-2)",
        )
        return None

    # -- writing back ----------------------------------------------------------------

    def finish(self):
        """The source with every edit applied, provided every group reads back;
        each note lands on its line in the text written."""
        converted = sources.apply(self.text, self.edits) if self.edits else self.text
        if self.edits:
            expected = [
                group.converted if group.converted is not None else group.text
                for group in self.extraction.groups
                if not group.interpolated
            ]
            found = [
                group.text
                for group in self.reader(converted).groups
                if not group.interpolated
            ]
            if expected != found:
                self.count("source files left after a failed read-back")
                self.note(
                    None,
                    "internal",
                    "a converted string does not read back as written, so the file"
                    " is left as it was",
                )
                self.edits, converted = [], self.text
        written = sources.Extraction(converted)
        for offset, kind, message in self.pending:
            line = 0
            if offset is not None:
                shift = sum(
                    len(text) - (end - start)
                    for start, end, text in self.edits
                    if end <= offset and (start, end) != (offset, offset)
                )
                line = written.line(offset + shift)
            self.result.notes.append(Note(self.label, line, kind, message))
        return converted
