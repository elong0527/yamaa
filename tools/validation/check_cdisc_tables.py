"""Keep the rule cards' tables of CDISC vocabulary values on the data file.

Rule cards hold tables whose cells list values of the closed CDISC
vocabularies in ``yaml/vocabulary/cdisc.yaml`` (#3030 P-VOC-1). The per-model
tables of SUB-3, SUB-10 and SUB-15 list each model's implementation guides,
each class group's dataset classes and models, and the origin type and source
pairs ``SDTM`` admits. SUB-9 lists the column roles, SUB-26 the form of each
date or time data type, and JSON-10 the Dataset-JSON type of each submission
data type. The cards are written by hand, so this check keeps each table on
the file:

- a cell lists values as code spans separated by commas, and each value a
  cell lists is a value of the vocabulary its column names;
- a column that places its vocabulary lists every value of it exactly once,
  except the values :data:`TABLES` says belong in no row, which it must not
  list;
- any other column lists a value at most once in a cell, and a cell of it
  may be prose instead, such as ``any``.

The file holds no per-model facts, so the row a value sits in is the card's
to state.
"""

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

from tools.paths import ROOT
from tools.rules.model import load, prose_lines
from tools.validation.check_cdisc_vocabulary import (
    VOCABULARY,
    document_errors,
    load_vocabulary,
    values,
)

# A value, written as a code span, and a cell that lists values.
VALUE = re.compile(r"`([^`]+)`")
VALUE_LIST = re.compile(r"`[^`]+`(?:, `[^`]+`)*")
SEPARATOR = re.compile(r":?-+:?")


@dataclass(frozen=True)
class Column:
    """A table column whose cells list values of one vocabulary.

    A column that ``places`` its vocabulary lists each value in one row,
    except the values ``unplaced`` names; any other column may repeat a value
    in several rows, and may hold prose.
    """

    vocabulary: str
    places: bool = True
    unplaced: tuple[str, ...] = ()


@dataclass(frozen=True)
class Table:
    """A card's table, found by its header row, with a column for each
    header cell: None for one that lists no vocabulary."""

    card: str
    header: tuple[str, ...]
    columns: tuple[Column | None, ...]


TABLES = (
    # Each model's implementation guides. BIMO is excluded, and CDISC/NCI
    # names controlled terminology, a standard of type CT: neither is a guide
    # of a model.
    Table(
        "SUB-3",
        ("Model", "Standard names"),
        (
            Column("model"),
            Column("standard_name", unplaced=("BIMO", "CDISC/NCI")),
        ),
    ),
    # What a column of each role is.
    Table("SUB-9", ("Role", "The column"), (Column("role"), None)),
    # Each group's classes and the models it serves.
    Table(
        "SUB-10",
        ("Group", "Classes", "Models"),
        (None, Column("dataset_class"), Column("model")),
    ),
    # The sources SDTM admits for each origin type. It admits no Not
    # Available origin, and admits any source for Other.
    Table(
        "SUB-15",
        ("Type", "Admitted sources for `SDTM`"),
        (
            Column("origin_type", unplaced=("Not Available",)),
            Column("origin_source", places=False),
        ),
    ),
    # The ISO 8601 form of each date or time data type; the others name none.
    Table(
        "SUB-26",
        ("`data_type`", "Form", "Examples"),
        (
            Column(
                "submission_data_type", unplaced=("text", "integer", "float", "URI")
            ),
            None,
            None,
        ),
    ),
    # The Dataset-JSON type each submission data type is written as.
    Table(
        "JSON-10",
        ("Submission data type", "`dataType`"),
        (Column("submission_data_type"), None),
    ),
)


def cells(line):
    """The cells of a table row, as written."""
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def card_tables(rule_file, card):
    """Each table in a card, as ``(line, cells)`` rows from its header on,
    without the separator row."""
    raw = rule_file.text.splitlines()
    masked = prose_lines(rule_file.text, code=True)
    end = min(
        (
            heading.line - 1
            for heading in rule_file.headings
            if heading.level <= 3 and heading.line > card.line
        ),
        default=len(raw),
    )
    tables = []
    rows = []
    for index in range(card.line, end):
        if masked[index].lstrip().startswith("|"):
            row = cells(raw[index])
            if not all(SEPARATOR.fullmatch(cell) for cell in row):
                rows.append((index + 1, row))
        elif rows:
            tables.append(rows)
            rows = []
    if rows:
        tables.append(rows)
    return tables


def column_errors(document, where, name, column, column_cells):
    """How one column's ``(line, cell)`` cells depart from its vocabulary.

    ``where`` gives the label of a line, and ``where(None)`` that of the
    header.
    """
    vocabulary = document["vocabularies"].get(column.vocabulary)
    spelled = f"{VOCABULARY}.vocabularies.{column.vocabulary}"
    if vocabulary is None:
        return [f"ERROR: {where(None)}: {name} lists {spelled}, which does not exist"]
    known = values(vocabulary)
    errors = []
    listed = set()
    for line, cell in column_cells:
        if VALUE_LIST.fullmatch(cell) is None:
            if column.places:
                errors.append(
                    f"ERROR: {where(line)}: {name} cell {cell!r} is not a list of "
                    "code spans"
                )
            continue
        in_cell = set()
        for value in VALUE.findall(cell):
            if value not in known:
                errors.append(
                    f"ERROR: {where(line)}: {name} lists `{value}`, which {spelled} "
                    "does not"
                )
            if value in in_cell or (column.places and value in listed):
                errors.append(f"ERROR: {where(line)}: {name} lists `{value}` twice")
            elif value in column.unplaced:
                errors.append(
                    f"ERROR: {where(line)}: {name} lists `{value}`, which belongs "
                    "in no row"
                )
            in_cell.add(value)
            listed.add(value)
    if column.places:
        for value in column.unplaced:
            if value not in known:
                errors.append(
                    f"ERROR: {where(None)}: `{value}` belongs in no row of {name}, "
                    f"but {spelled} does not list it"
                )
        for value in known:
            if value not in listed and value not in column.unplaced:
                errors.append(
                    f"ERROR: {where(None)}: {name} does not list `{value}`, which "
                    f"{spelled} lists"
                )
    return errors


def table_errors(document, rules, table):
    """How one card's table departs from the vocabularies its columns list."""
    card = rules.resolve(table.card)
    if card is None:
        return [f"ERROR: rules: no card {table.card}, whose table this check reads"]
    found = [
        rows
        for rows in card_tables(rules.files[card.file], card)
        if rows[0][1] == list(table.header)
    ]
    label = f"rules/{card.file}"
    if not found:
        heading = " | ".join(table.header)
        return [f"ERROR: {label}:{card.line}: {card.id}: no table headed | {heading} |"]
    (header_line, _), *rows = found[0]

    def where(line):
        return f"{label}:{header_line if line is None else line}: {card.id}"

    errors = [
        f"ERROR: {where(line)}: the row and the header have {len(row)} and "
        f"{len(table.header)} cells"
        for line, row in rows
        if len(row) != len(table.header)
    ]
    rows = [(line, row) for line, row in rows if len(row) == len(table.header)]
    for position, column in enumerate(table.columns):
        if column is not None:
            errors.extend(
                column_errors(
                    document,
                    where,
                    table.header[position],
                    column,
                    [(line, row[position]) for line, row in rows],
                )
            )
    return errors


def check(root):
    """Every way the tables depart from the data file."""
    root = Path(root)
    document, errors = load_vocabulary(root)
    if document is None:
        return errors
    errors = document_errors(document)
    if errors:
        return errors
    rules = load(root)
    return [error for table in TABLES for error in table_errors(document, rules, table)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    errors = check(parser.parse_args().root)
    for error in errors:
        print(error)
    if errors:
        return 1
    cards = [table.card for table in TABLES]
    print(
        f"PASS: the tables of {', '.join(cards[:-1])} and {cards[-1]} list the "
        f"values of {VOCABULARY}."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
