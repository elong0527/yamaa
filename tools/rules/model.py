"""One reading of the rule files, shared by every repository tool.

Rule files are ``rules/**/*.md`` except the index (``rules/README.md``), the
informative glossary and the reference pages the docs build generates. A rule
file holds cards headed ``### PREFIX-N. Title``. The index opens with the
principles and lists every rule file with its prefix in the Rule files table.

Parsing never fails on malformed rules. :mod:`tools.rules.check` reports what
is malformed; other tools read the identities they need.
"""

import os
import posixpath
import re
import subprocess
import unicodedata
from dataclasses import dataclass
from pathlib import Path

METADATA_LABELS = ("Fails", "Evidence")
# Rules-relative pages under rules/ that hold no rules.
INDEX = "README.md"
INFORMATIVE = frozenset({"reference/glossary.md"})
# Rendered from rules/ and yaml/ on every docs build; never committed.
RULE_INDEX = "reference/rule-index.md"
SCHEMA_FIELDS = "reference/schema-fields.md"
RULES_SUMMARY = "reference/rules-summary.md"
GENERATED_PAGES = (RULE_INDEX, SCHEMA_FIELDS, RULES_SUMMARY)
# Directories that hold build output, environments or caches, never sources.
SKIPPED_DIRECTORIES = frozenset(
    {
        ".build",
        ".cache",
        ".git",
        ".pytest_cache",
        ".ruff_cache",
        ".venv",
        ".venv-docs",
        ".worktrees",
        "__pycache__",
        "node_modules",
        "site",
        "target",
        "venv",
    }
)
# Root-relative generated output that Git ignores.
SKIPPED_PATHS = frozenset(
    {"build", "docs/benchmark", "docs/coverage"}
    | {f"rules/{page}" for page in GENERATED_PAGES}
)

CARD_HEADING = re.compile(
    r"^###\s+(?P<id>(?P<prefix>[A-Z]+)-(?P<number>\d+))(?P<dot>\.?)"
    r"(?:\s+(?P<title>.*?))?\s*$"
)
ANCHOR = re.compile(r'^<a id="(?P<anchor>[^"]*)"></a>\s*$')
HEADING = re.compile(r"^ {0,3}(?P<marks>#{1,6})(?:\s+(?P<text>.*?))?(?:\s+#+)?\s*$")
METADATA_LINE = re.compile(rf"^(?P<label>{'|'.join(METADATA_LABELS)}):(?P<value>.*)$")
PRINCIPLE = re.compile(r"^\*\*P(?P<number>\d+)\.\s+(?P<title>.+?)\*\*\s*(?P<text>.*)$")
LISTING_HEADER = ["File", "Prefix", "Covers"]
LISTING_LINK = re.compile(r"\[(?P<title>[^\]]+)\]\((?P<target>[^)\s]+)\)")

FRONT_MATTER = re.compile(r"\A---\n.*?\n---\n", re.DOTALL)
FENCE_OPEN = re.compile(r"^(?P<head>[^`~]*?)(?P<fence>`{3,}|~{3,})(?P<info>[^`~]*)$")
FENCE_CLOSE = re.compile(r"^\s*(?P<fence>`{3,}|~{3,})\s*$")
CODE_SPAN = re.compile(r"(?<!`)(?P<run>`+)(?!`).+?(?<!`)(?P=run)(?!`)")
LINK = re.compile(
    r"(?<![!\\])\[(?P<text>(?:[^\[\]\n]|\[[^\[\]\n]*\])*)\]"
    r"\((?P<target><[^<>\n]*>|[^()\s<>]*(?:\([^()\s]*\)[^()\s<>]*)*)"
    r"(?:\s+(?:\"[^\"\n]*\"|'[^'\n]*'))?\)"
)
# A link reference definition; a footnote ("[^1]: text") is not one.
REFERENCE_DEFINITION = re.compile(
    r"^ {0,3}\[(?P<text>[^\]\n^][^\]\n]*)\]:\s*(?P<target><[^<>\n]*>|\S+)"
)
EXPLICIT_ANCHOR = re.compile(r"<a\s[^>]*?\b(?:id|name)=\"(?P<anchor>[^\"]+)\"", re.I)
ATTRIBUTE_ID = re.compile(r"\s*\{[^{}]*#(?P<anchor>[\w-]+)[^{}]*\}\s*$")


class RuleSourceError(ValueError):
    """A rule source that cannot be read, such as an unknown Git revision."""


# -- markdown -----------------------------------------------------------------


def prose_lines(text, *, code=False):
    """Return a markdown text's lines with everything but prose blanked.

    Front matter and fenced blocks become empty lines and HTML comments become
    spaces, so line numbers and columns stay true. A fence may open after prose
    on its line; that prose remains. Inline code spans become spaces unless
    ``code`` is true.
    """
    front = FRONT_MATTER.match(text)
    skipped = front[0].count("\n") if front else 0
    result = []
    fence = None
    comment = False
    for number, line in enumerate(text.splitlines()):
        if number < skipped:
            result.append("")
            continue
        if fence is not None:
            closing = FENCE_CLOSE.match(line)
            if (
                closing
                and closing["fence"][0] == fence[0]
                and len(closing["fence"]) >= len(fence)
            ):
                fence = None
            result.append("")
            continue
        line, comment = _blank_comments(line, comment)
        opening = FENCE_OPEN.match(line)
        if opening:
            fence = opening["fence"]
            line = opening["head"] if opening["head"].strip() else ""
        if not code:
            line = CODE_SPAN.sub(lambda span: " " * len(span[0]), line)
        result.append(line)
    return result


def _blank_comments(line, open_comment):
    """Blank HTML comment text on one line; report whether one stays open."""
    parts = []
    offset = 0
    while offset < len(line):
        if open_comment:
            end = line.find("-->", offset)
            stop = len(line) if end < 0 else end + 3
            parts.append(" " * (stop - offset))
            offset = stop
            open_comment = end < 0
        else:
            start = line.find("<!--", offset)
            if start < 0:
                parts.append(line[offset:])
                break
            parts.append(line[offset:start] + " " * 4)
            offset = start + 4
            open_comment = True
    return "".join(parts), open_comment


@dataclass(frozen=True)
class Link:
    """One inline link or reference definition, located in its source line.

    ``start``/``end`` span the whole link, ``text_*`` its text and
    ``target_*`` its target as written, angle brackets included.
    """

    line: int
    start: int
    end: int
    text: str
    target: str
    text_start: int
    text_end: int
    target_start: int
    target_end: int

    @property
    def path(self):
        """The link target without its fragment or query."""
        return self.target.split("#", 1)[0].split("?", 1)[0]

    @property
    def fragment(self):
        return self.target.split("#", 1)[1] if "#" in self.target else ""

    @property
    def external(self):
        """True for URLs, mail and root-absolute targets; False for repository paths."""
        return bool(re.match(r"[A-Za-z][A-Za-z0-9+.-]*:|/", self.target))


def edit_lines(text, edits):
    """Apply ``(line, start, end, replacement)`` edits to a text.

    Columns are those of :func:`markdown_links`; edits on one line must not
    overlap. Line endings are kept.
    """
    lines = text.splitlines(keepends=True)
    for line, start, end, replacement in sorted(edits, reverse=True):
        content = lines[line - 1]
        lines[line - 1] = content[:start] + replacement + content[end:]
    return "".join(lines)


def markdown_links(text):
    """Return the inline links and reference definitions outside code."""
    raw = text.splitlines()
    result = []
    for number, line in enumerate(prose_lines(text), 1):
        source = raw[number - 1]
        definition = REFERENCE_DEFINITION.match(line)
        matches = [definition] if definition else LINK.finditer(line)
        for match in matches:
            target = source[match.start("target") : match.end("target")]
            result.append(
                Link(
                    line=number,
                    start=match.start(),
                    end=match.end(),
                    text=source[match.start("text") : match.end("text")],
                    target=target.removeprefix("<").removesuffix(">"),
                    text_start=match.start("text"),
                    text_end=match.end("text"),
                    target_start=match.start("target"),
                    target_end=match.end("target"),
                )
            )
    return result


def heading_text(text):
    """The text a heading renders, without its markup."""
    text = ATTRIBUTE_ID.sub("", text)
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"<[^>]+>", "", text).replace("`", "")
    return re.sub(r"\*+", "", text).strip()


def toc_slug(text):
    """The anchor the site's Markdown table of contents gives a heading."""
    value = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    value = re.sub(r"[^\w\s-]", "", value).strip().lower()
    return re.sub(r"[-\s]+", "-", value)


def github_slug(text):
    """The anchor GitHub gives a heading."""
    return re.sub(r"[^\w\- ]", "", text.strip().lower()).replace(" ", "-")


def anchors(text):
    """Every anchor a markdown text defines: explicit ids and heading slugs.

    Both the site's and GitHub's heading anchors count, including their
    suffixes for repeated headings, because rule files render in both.
    """
    lines = prose_lines(text, code=True)
    found = set()
    toc_seen = set()
    github_seen = {}
    for line in lines:
        found.update(match["anchor"] for match in EXPLICIT_ANCHOR.finditer(line))
        heading = HEADING.match(line)
        if heading is None or not heading["text"]:
            continue
        attribute = ATTRIBUTE_ID.search(heading["text"])
        if attribute:
            found.add(attribute["anchor"])
            continue
        rendered = heading_text(heading["text"])
        slug = toc_slug(rendered)
        unique = slug
        while unique in toc_seen or not unique:
            counted = re.match(r"^(.*)_([0-9]+)$", unique)
            unique = f"{counted[1]}_{int(counted[2]) + 1}" if counted else f"{unique}_1"
        toc_seen.add(unique)
        found.add(unique)
        slug = github_slug(rendered)
        count = github_seen.get(slug, 0)
        github_seen[slug] = count + 1
        found.add(slug if count == 0 else f"{slug}-{count}")
    return found


def words(text):
    """Count the words a paragraph renders; a link counts as its text."""
    return len(LINK.sub(lambda match: match["text"], text).split())


# -- rule files ---------------------------------------------------------------


@dataclass(frozen=True)
class Heading:
    level: int
    text: str
    line: int


@dataclass(frozen=True)
class Metadata:
    """One ``Fails:`` or ``Evidence:`` entry of a card.

    An entry may wrap; ``line`` and ``end`` are its first and last lines.
    """

    label: str
    value: str
    line: int
    end: int

    @property
    def items(self):
        """Comma-separated items as written, without the final period."""
        value = self.value.strip().removesuffix(".")
        return tuple(item.strip() for item in value.split(",")) if value else ()

    @property
    def names(self):
        """Items with one enclosing code span removed."""
        return tuple(
            item[1:-1] if len(item) > 1 and item[0] == item[-1] == "`" else item
            for item in self.items
        )


@dataclass(frozen=True)
class Card:
    id: str
    prefix: str
    number: str
    file: str
    line: int
    anchor: str | None
    title: str
    dotted: bool
    topic: str | None
    statement: str | None
    statement_line: int | None
    metadata: tuple[Metadata, ...]
    detail: str
    stray_metadata: tuple[int, ...]

    def entry(self, label):
        return next((item for item in self.metadata if item.label == label), None)

    def names(self, label):
        return () if (item := self.entry(label)) is None else item.names

    @property
    def fails(self):
        return self.names("Fails")

    @property
    def evidence(self):
        return self.names("Evidence")


@dataclass(frozen=True)
class RuleFile:
    path: str
    text: str
    title: str | None
    front_matter: str | None
    headings: tuple[Heading, ...]
    cards: tuple[Card, ...]
    stray_headings: tuple[Heading, ...]


@dataclass(frozen=True)
class Listing:
    """One row of the index's Rule files table."""

    path: str
    title: str
    prefix: str
    covers: str
    line: int


@dataclass(frozen=True)
class Principle:
    number: int
    title: str
    text: str
    line: int


def is_rule_file(relative):
    """Whether a rules-relative markdown path holds rules."""
    return (
        relative.endswith(".md")
        and relative != INDEX
        and relative not in INFORMATIVE
        and relative not in GENERATED_PAGES
    )


def _previous_anchor(lines, index):
    """The anchor on the nearest non-blank line above ``index``, if any."""
    index -= 1
    while index >= 0 and not lines[index].strip():
        index -= 1
    if index < 0:
        return None, None
    anchor = ANCHOR.match(lines[index])
    return (anchor["anchor"], index) if anchor else (None, None)


def _blocks(raw, start, end):
    """Split lines into paragraphs: runs of lines that are not blank."""
    blocks = []
    index = start
    while index < end:
        if not raw[index].strip():
            index += 1
            continue
        first = index
        while index < end and raw[index].strip():
            index += 1
        blocks.append((first, index))
    return blocks


def _is_paragraph(raw_line, masked_line):
    """Whether a block's first line opens a paragraph, not a table or list."""
    return bool(masked_line.strip()) and not re.match(
        r"\s*(?:[|#<>]|[*+-]\s|\d+[.)]\s|`{3,}|~{3,})", raw_line
    )


def _parse_card(path, raw, masked, heading_index, end, topic):
    heading = CARD_HEADING.match(masked[heading_index])
    anchor, _ = _previous_anchor(masked, heading_index)
    blocks = _blocks(raw, heading_index + 1, end)
    statement = statement_line = None
    metadata = []
    stray = []
    rest = blocks
    if blocks and _is_paragraph(raw[blocks[0][0]], masked[blocks[0][0]]):
        first, stop = blocks[0]
        if not METADATA_LINE.match(raw[first]):
            statement = "\n".join(raw[first:stop])
            statement_line = first + 1
            stray.extend(
                number + 1
                for number in range(first, stop)
                if METADATA_LINE.match(masked[number])
            )
            rest = blocks[1:]
    if rest and METADATA_LINE.match(masked[rest[0][0]]):
        first, stop = rest[0]
        entries = []
        for number in range(first, stop):
            match = METADATA_LINE.match(raw[number])
            if match:
                entries.append(
                    [match["label"], match["value"].strip(), number + 1, number + 1]
                )
            else:
                entries[-1][1] = f"{entries[-1][1]} {raw[number].strip()}".strip()
                entries[-1][3] = number + 1
        metadata = [Metadata(*entry) for entry in entries]
        rest = rest[1:]
    detail_start = rest[0][0] if rest else end
    stray.extend(
        number + 1
        for number in range(detail_start, end)
        if METADATA_LINE.match(masked[number])
    )
    return Card(
        id=heading["id"],
        prefix=heading["prefix"],
        number=heading["number"],
        file=path,
        line=heading_index + 1,
        anchor=anchor,
        title=heading["title"] or "",
        dotted=bool(heading["dot"]),
        topic=topic,
        statement=statement,
        statement_line=statement_line,
        metadata=tuple(metadata),
        detail="\n".join(raw[detail_start:end]).strip("\n"),
        stray_metadata=tuple(stray),
    )


def parse_file(path, text):
    """Parse one rule file's cards."""
    raw = text.splitlines()
    masked = prose_lines(text, code=True)
    front = FRONT_MATTER.match(text)
    headings = []
    for number, line in enumerate(masked):
        match = HEADING.match(line)
        if match:
            headings.append(
                Heading(len(match["marks"]), (match["text"] or "").strip(), number + 1)
            )
    title = next((item.text for item in headings if item.level == 1), None)
    boundaries = sorted(item.line - 1 for item in headings if item.level <= 3)
    card_indexes = [index for index in boundaries if CARD_HEADING.match(masked[index])]
    starts = {}
    for index in card_indexes:
        _, anchor_index = _previous_anchor(masked, index)
        starts[index] = index if anchor_index is None else anchor_index
    cards = []
    for index in card_indexes:
        later = [line for line in boundaries if line > index]
        later += [starts[card] for card in card_indexes if card > index]
        end = min(later, default=len(raw))
        topic = next(
            (
                item.text
                for item in reversed(headings)
                if item.level == 2 and item.line - 1 < index
            ),
            None,
        )
        cards.append(_parse_card(path, raw, masked, index, end, topic))
    stray_headings = tuple(
        item
        for item in headings
        if item.level == 3 and not CARD_HEADING.match(masked[item.line - 1])
    )
    return RuleFile(
        path=path,
        text=text,
        title=title,
        front_matter=front[0] if front else None,
        headings=tuple(headings),
        cards=tuple(cards),
        stray_headings=stray_headings,
    )


# -- the index ----------------------------------------------------------------


def _section(lines, title):
    """Line indexes of a ``## title`` section's body, or None."""
    start = next(
        (index for index, line in enumerate(lines) if line.strip() == f"## {title}"),
        None,
    )
    if start is None:
        return None
    end = next(
        (
            index
            for index in range(start + 1, len(lines))
            if re.match(r"^#{1,2}\s", lines[index])
        ),
        len(lines),
    )
    return range(start + 1, end)


def _cells(line):
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def parse_listing(text):
    """Read the Rule files table: rows and the problems of malformed rows.

    Returns ``(None, [])`` when the index has no Rule files table.
    """
    lines = prose_lines(text, code=True)
    body = _section(lines, "Rule files")
    if body is None:
        return None, []
    table = [index for index in body if lines[index].lstrip().startswith("|")]
    if not table or _cells(lines[table[0]]) != LISTING_HEADER:
        return None, []
    rows = []
    problems = []
    if len(table) < 2 or not all(
        re.fullmatch(r":?-+:?", cell) for cell in _cells(lines[table[1]])
    ):
        problems.append((table[0] + 1, "the Rule files table needs a separator row"))
    for index in table[2:]:
        cells = _cells(lines[index])
        link = LISTING_LINK.fullmatch(cells[0]) if cells else None
        if len(cells) != 3 or link is None:
            problems.append(
                (index + 1, "expected | [Title](file.md) | PREFIX | Covers |")
            )
            continue
        rows.append(
            Listing(
                path=posixpath.normpath(link["target"].split("#", 1)[0]),
                title=link["title"],
                prefix=cells[1],
                covers=cells[2],
                line=index + 1,
            )
        )
    return rows, problems


def parse_principles(text):
    """Read the principles: bold ``**Pn. Title.**`` paragraphs of the index."""
    raw = text.splitlines()
    lines = prose_lines(text, code=True)
    body = _section(lines, "Principles")
    if body is None:
        return []
    principles = []
    for first, stop in _blocks(raw, body.start, body.stop):
        match = PRINCIPLE.match(raw[first])
        if match:
            principles.append(
                Principle(
                    number=int(match["number"]),
                    title=match["title"].strip(),
                    text="\n".join([match["text"], *raw[first + 1 : stop]]).strip(),
                    line=first + 1,
                )
            )
    return principles


# -- the rule set -------------------------------------------------------------


class RuleSet:
    """Every rule file, the index listing and principles, and ID resolution."""

    def __init__(self, files, readme=None):
        self.files = files
        self.readme = readme
        if readme is None:
            self.listing, self.listing_problems = None, []
            self.principles = []
        else:
            self.listing, self.listing_problems = parse_listing(readme)
            self.principles = parse_principles(readme)
        self.cards = {}
        for rule_file in files.values():
            for card in rule_file.cards:
                self.cards.setdefault(card.id, card)
        declared = {row.prefix for row in self.listing or ()}
        declared |= {card.prefix for card in self.all_cards()}
        self.prefixes = frozenset(
            prefix for prefix in declared if re.fullmatch(r"[A-Z]+", prefix)
        )
        names = "|".join(sorted(self.prefixes, key=lambda item: (-len(item), item)))
        # Without a prefix no text cites a rule: the pattern never matches.
        pattern = rf"(?:{names})-\d+" if names else r"(?!)"
        self.citation = re.compile(rf"\b{pattern}\b")
        self.identifier = re.compile(pattern)

    def all_cards(self):
        """Every card in file order, repeated IDs included."""
        return [card for item in self.files.values() for card in item.cards]

    def ordered_files(self):
        """Rule files in index order, then any unlisted file by path."""
        listed = [row.path for row in self.listing or () if row.path in self.files]
        return [self.files[path] for path in dict.fromkeys(listed)] + [
            self.files[path] for path in sorted(self.files) if path not in listed
        ]

    def identifiers(self):
        """Every ID a citation may name: the card IDs."""
        return set(self.cards)

    def resolve(self, identifier):
        """The card an ID names, or None."""
        return self.cards.get(identifier)

    def location(self, identifier):
        """The rules-relative file and anchor that define an ID, or None."""
        card = self.resolve(identifier)
        return None if card is None else (card.file, card.id.lower())

    def citations(self, text):
        """Yield ``(line, column, id)`` for every rule ID cited in a text."""
        for number, line in enumerate(text.splitlines(), 1):
            for match in self.citation.finditer(line):
                yield number, match.start(), match[0]


def parse(texts, readme=None):
    """Build a rule set from rules-relative paths and their texts."""
    files = {path: parse_file(path, texts[path]) for path in sorted(texts)}
    return RuleSet(files, readme)


def load(root):
    """Read the rule files of a checkout."""
    directory = Path(root) / "rules"
    texts = {}
    if directory.is_dir():
        for path in sorted(directory.rglob("*.md")):
            relative = path.relative_to(directory).as_posix()
            if is_rule_file(relative):
                texts[relative] = path.read_text(encoding="utf-8")
    index = directory / INDEX
    return parse(texts, index.read_text(encoding="utf-8") if index.is_file() else None)


def _git(root, *arguments):
    result = subprocess.run(
        ["git", "-C", str(root), *arguments],
        capture_output=True,
        check=False,
    )
    if result.returncode:
        message = result.stderr.decode("utf-8", "replace").strip()
        raise RuleSourceError(f"git {' '.join(arguments)}: {message}")
    return result.stdout.decode("utf-8")


def load_ref(root, ref):
    """Read the rule files of a Git revision."""
    names = _git(root, "ls-tree", "-r", "-z", "--name-only", ref, "--", "rules")
    texts = {}
    readme = None
    for name in filter(None, names.split("\0")):
        relative = name.removeprefix("rules/")
        if relative == INDEX:
            readme = _git(root, "cat-file", "blob", f"{ref}:{name}")
        elif is_rule_file(relative):
            texts[relative] = _git(root, "cat-file", "blob", f"{ref}:{name}")
    return parse(texts, readme)


def authored_files(root, suffixes=None, names=()):
    """Repository files outside build output, environments and generated pages.

    ``suffixes`` (lowercase, with dots) and ``names`` select files; None
    selects every file.
    """
    root = Path(root)
    result = []
    for directory, subdirectories, files in os.walk(root):
        base = Path(directory).relative_to(root)
        subdirectories[:] = sorted(
            name
            for name in subdirectories
            if name not in SKIPPED_DIRECTORIES
            and not name.endswith(".egg-info")
            and (base / name).as_posix() not in SKIPPED_PATHS
        )
        for name in sorted(files):
            relative = (base / name).as_posix()
            if relative in SKIPPED_PATHS:
                continue
            if (
                suffixes is None
                or Path(name).suffix.lower() in suffixes
                or name in names
            ):
                result.append(Path(directory) / name)
    return result
