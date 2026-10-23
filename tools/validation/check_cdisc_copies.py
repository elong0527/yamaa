"""Keep the lists of CDISC vocabulary values outside the cards' tables on the data file.

Rule cards, site pages and one tool list values of the closed CDISC
vocabularies in ``yaml/vocabulary/cdisc.yaml`` (#3030 P-VOC-1) by hand, outside
the tables ``check_cdisc_tables`` reads. This check keeps each list on the
file:

- a passage of a card or a page, from the words that open it to the words that
  close it or else to the end of its sentence, names in code spans only values
  of the vocabulary it lists or of the vocabularies :data:`PASSAGES` lets it
  also name. It names every value of its vocabulary except those it leaves
  out, which it must not name;
- the mapping doc's ``TEMPORAL_SUBMISSION_TYPES`` holds exactly the submission
  data types that have a date or time form: every one but those SUB-26's table
  places in no row.

The file holds no per-model facts, so which model a value goes with is the
passage's to state.
"""

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

from tools.benchmark_docs import mapping_doc
from tools.paths import ROOT
from tools.rules.model import load
from tools.validation.check_cdisc_tables import TABLES, VALUE
from tools.validation.check_cdisc_vocabulary import (
    VOCABULARY,
    document_errors,
    load_vocabulary,
    values,
)

CARD = re.compile(r"[A-Z]+-[0-9]+")
# A code span, or a period that ends a sentence.
SENTENCE = re.compile(r"`[^`]*`|\.(?=\s|$)")
MAPPING_DOC = Path("tools/benchmark_docs/mapping_doc.py")


@dataclass(frozen=True)
class Passage:
    """Prose that lists values of one vocabulary.

    ``place`` is a card ID or a page's path. The passage follows ``start``,
    which opens it once in that card or page, and runs to ``end`` or, without
    one, to the end of the sentence. It may also name values of the
    vocabularies in ``also``, and names none of those in ``omitted``.
    """

    place: str
    start: str
    vocabulary: str
    end: str | None = None
    also: tuple[str, ...] = ()
    omitted: tuple[str, ...] = ()


PASSAGES = (
    # The types a `str` column admits besides `text`, its default: all but
    # the two numeric ones.
    Passage(
        "SUB-6",
        "`str` admits",
        "submission_data_type",
        omitted=("text", "integer", "float"),
    ),
    # The designations each model publishes, in the statement and again with
    # their meanings.
    Passage(
        "SUB-10",
        "is declared for every model:",
        "core_designation",
        also=("model",),
    ),
    Passage(
        "SUB-10",
        "For `SDTM` and `SEND`,",
        "core_designation",
        end="as the ADaM implementation guides publish them",
        also=("model",),
    ),
    Passage(
        "SUB-14",
        "Each origin's `type` is one of",
        "origin_type",
        end="and its `source`",
    ),
    Passage("SUB-14", "and its `source` one of", "origin_source"),
    Passage("CT-2", "its publishing set (", "publishing_set", end=")"),
    Passage("DEFINE-6", "A guide's `status`,", "standard_status", end=", is its"),
    # The page on spreadsheet specifications gives each model's designations
    # beside the Core column, in a list item and in a table row.
    Passage(
        "docs/articles/excel.md",
        "It travels in `submission.core` (",
        "core_designation",
        end=")",
    ),
    Passage("docs/articles/excel.md", "| `Core` (", "core_designation", end=")"),
    Passage("docs/articles/terminology.md", "in their exact case:", "standard_name"),
    Passage(
        "docs/articles/terminology.md",
        "`publishing_set` is one of",
        "publishing_set",
        end=", and `version`",
    ),
)


def phrase(words):
    """The pattern of ``words``, however a line wraps them."""
    return re.compile(r"\s+".join(re.escape(word) for word in words.split()))


def line_of(text, offset):
    """The one-based line of an offset into ``text``."""
    return text.count("\n", 0, offset) + 1


def place_text(root, rules, place):
    """The text of a card or a page, its file, the line it starts on and
    its label; or None with the error when it is gone."""
    if CARD.fullmatch(place):
        card = rules.resolve(place)
        if card is None:
            return None, f"ERROR: rules: no card {place}, whose list this check reads"
        rule_file = rules.files[card.file]
        lines = rule_file.text.splitlines(keepends=True)
        end = min(
            (
                heading.line - 1
                for heading in rule_file.headings
                if heading.level <= 3 and heading.line > card.line
            ),
            default=len(lines),
        )
        text = "".join(lines[card.line - 1 : end])
        return (text, f"rules/{card.file}", card.line, card.id), None
    path = Path(root) / place
    if not path.is_file():
        return None, f"ERROR: {place}: no such page, whose list this check reads"
    return (path.read_text(encoding="utf-8"), place, 1, None), None


def passage_errors(document, root, rules, passage):
    """How one passage departs from the vocabularies it names."""
    found, error = place_text(root, rules, passage.place)
    if found is None:
        return [error]
    text, file, first, card = found

    def where(offset):
        line = first + line_of(text, offset) - 1
        return f"{file}:{line}" + (f": {card}" if card else "")

    vocabularies = document["vocabularies"]
    spelled = f"{VOCABULARY}.vocabularies.{passage.vocabulary}"
    missing = [
        name for name in (passage.vocabulary, *passage.also) if name not in vocabularies
    ]
    if missing:
        return [
            f"ERROR: {where(0)}: the list after {passage.start!r} names "
            f"{VOCABULARY}.vocabularies.{name}, which does not exist"
            for name in missing
        ]
    opened = list(phrase(passage.start).finditer(text))
    if len(opened) != 1:
        return [
            f"ERROR: {where(0)}: {len(opened)} passages open with {passage.start!r}"
        ]
    begin = opened[0].end()
    if passage.end is None:
        stop = next(
            (
                match.start()
                for match in SENTENCE.finditer(text, begin)
                if match[0] == "."
            ),
            len(text),
        )
    else:
        closed = phrase(passage.end).search(text, begin)
        if closed is None:
            return [
                f"ERROR: {where(begin)}: the list after {passage.start!r} does not "
                f"end with {passage.end!r}"
            ]
        stop = closed.start()
    label = f"the list after {passage.start!r}"
    known = values(vocabularies[passage.vocabulary])
    others = {value for name in passage.also for value in values(vocabularies[name])}
    errors = []
    listed = set()
    for match in VALUE.finditer(text, begin, stop):
        value = " ".join(match[1].split())
        if value in passage.omitted:
            errors.append(
                f"ERROR: {where(match.start())}: {label} names `{value}`, which it "
                "leaves out"
            )
        elif value not in known and value not in others:
            errors.append(
                f"ERROR: {where(match.start())}: {label} names `{value}`, which "
                f"{spelled} does not"
            )
        listed.add(value)
    for value in passage.omitted:
        if value not in known:
            errors.append(
                f"ERROR: {where(begin)}: {label} leaves out `{value}`, but {spelled} "
                "does not list it"
            )
    for value in known:
        if value not in listed and value not in passage.omitted:
            errors.append(
                f"ERROR: {where(begin)}: {label} does not name `{value}`, which "
                f"{spelled} lists"
            )
    return errors


def temporal_errors(document):
    """How the mapping doc's date and time types depart from the data file."""
    column = next(table for table in TABLES if table.card == "SUB-26").columns[0]
    spelled = f"{VOCABULARY}.vocabularies.{column.vocabulary}"
    expected = [
        value
        for value in values(document["vocabularies"][column.vocabulary])
        if value not in column.unplaced
    ]
    held = mapping_doc.TEMPORAL_SUBMISSION_TYPES
    label = f"ERROR: {MAPPING_DOC}: TEMPORAL_SUBMISSION_TYPES"
    return [
        f"{label} lacks `{value}`, a date or time type of {spelled} (SUB-26)"
        for value in expected
        if value not in held
    ] + [
        f"{label} holds `{value}`, which is no date or time type of {spelled} (SUB-26)"
        for value in sorted(held)
        if value not in expected
    ]


def check(root):
    """Every way the lists depart from the data file."""
    root = Path(root)
    document, errors = load_vocabulary(root)
    if document is None:
        return errors
    errors = document_errors(document)
    if errors:
        return errors
    rules = load(root)
    return [
        *(
            error
            for passage in PASSAGES
            for error in passage_errors(document, root, rules, passage)
        ),
        *temporal_errors(document),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    errors = check(parser.parse_args().root)
    for error in errors:
        print(error)
    if errors:
        return 1
    places = list(dict.fromkeys(passage.place for passage in PASSAGES))
    print(
        f"PASS: the lists of {', '.join(places)} and {MAPPING_DOC} name the "
        f"values of {VOCABULARY}."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
