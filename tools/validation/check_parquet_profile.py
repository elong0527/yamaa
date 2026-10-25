"""Keep PARQUET-7's table on the closed Parquet profile's data file.

The engine's Parquet reader reads its closed profile from
``rust/crates/yamaa-adapters/profile/parquet.toml``: each ``column`` row
names a physical type, the annotations that read as a column type (a
logical type from ``logical``, or none with ``unannotated``), and the Arrow
layouts. PARQUET-7 states the same closed mapping as a table of Parquet types
and column types, written by hand, so this check keeps the two equal: every
pair of a physical type and an annotation that the table reads as a column
type is one the data file reads as that type, and the other way round.

A table cell names its physical type as the first code span, then either
``with no annotation`` or ``annotated`` and its annotations as code spans. A
timestamp annotation's unit and UTC adjustment follow it as prose, such as
```Timestamp`, microseconds, no UTC adjustment``, which the data file spells
``Timestamp(MICROS, isAdjustedToUTC=false)``. The converted types and layouts
the rows also list are the card's prose, which this check does not read.
"""

import argparse
import re
import sys
import tomllib
from pathlib import Path

from tools.paths import ROOT
from tools.rules.model import load, prose_lines

PROFILE = Path("rust/crates/yamaa-adapters/profile/parquet.toml")
CARD = "PARQUET-7"
HEADER = ["Parquet type", "Column type"]
SPAN = re.compile(r"`([^`]+)`")
SEPARATOR = re.compile(r":?-+:?")
UNITS = {"milliseconds": "MILLIS", "microseconds": "MICROS", "nanoseconds": "NANOS"}
ADJUSTMENTS = {"no UTC adjustment": "false", "adjusted to UTC": "true"}
# What the data file spells an absent annotation as, here.
NONE = "none"


def cells(line):
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def card_rows(rules):
    """PARQUET-7's table rows below its header, as ``(line, cells)``; or the
    error that it has none."""
    card = rules.resolve(CARD)
    if card is None:
        return None, [f"ERROR: rules: no card {CARD}, whose table this check reads"]
    rule_file = rules.files[card.file]
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
    rows, found = [], False
    for index in range(card.line, end):
        if not masked[index].lstrip().startswith("|"):
            if found:
                break
            continue
        row = cells(raw[index])
        if row == HEADER:
            found = True
        elif found and not all(SEPARATOR.fullmatch(cell) for cell in row):
            rows.append((index + 1, row))
    label = f"rules/{card.file}"
    if not found:
        heading = " | ".join(HEADER)
        return None, [
            f"ERROR: {label}:{card.line}: {CARD}: no table headed | {heading} |"
        ]
    return (label, rows), []


def annotation(name, qualifiers):
    """A card's annotation in the data file's spelling: a timestamp takes its
    unit and UTC adjustment from the prose after it."""
    if name != "Timestamp":
        return name if not qualifiers else None
    unit = [UNITS[word] for word in qualifiers if word in UNITS]
    adjusted = [ADJUSTMENTS[word] for word in qualifiers if word in ADJUSTMENTS]
    if len(unit) != 1 or len(adjusted) != 1 or len(qualifiers) != 2:
        return None
    return f"Timestamp({unit[0]}, isAdjustedToUTC={adjusted[0]})"


def parse_type(cell):
    """The physical type and annotations a Parquet type cell names, or None.

    The annotations are code spans that commas or ``or`` separate; prose
    after the last one qualifies it.
    """
    spans = list(SPAN.finditer(cell))
    if not spans:
        return None
    physical = spans[0].group(1)
    rest = cell[spans[0].end() :].strip()
    if rest == "with no annotation":
        return physical, [NONE]
    if not rest.startswith("annotated ") or len(spans) < 2:
        return None
    listed = spans[1:]
    for left, right in zip(listed, listed[1:]):
        if cell[left.end() : right.start()].strip() not in {",", ", or", "or"}:
            return None
    tail = cell[listed[-1].end() :].split(",")
    qualifiers = [part.strip() for part in tail if part.strip()]
    names = [span.group(1) for span in listed[:-1]]
    if any(annotation(name, []) is None for name in names):
        return None
    last = annotation(listed[-1].group(1), qualifiers)
    if last is None:
        return None
    return physical, [*names, last]


def profile_pairs(document):
    """Each ``(physical, annotation, type)`` the data file's column rows read."""
    pairs = set()
    for row in document.get("column", []):
        annotations = list(row.get("logical", []))
        if row.get("unannotated"):
            annotations.append(NONE)
        pairs.update((row["physical"], name, row["type"]) for name in annotations)
    return pairs


def check(root):
    """Every way PARQUET-7's table departs from the data file."""
    root = Path(root)
    try:
        document = tomllib.loads((root / PROFILE).read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        return [f"ERROR: {PROFILE}: {error}"]
    found, errors = card_rows(load(root))
    if found is None:
        return errors
    label, rows = found
    table = {}
    for line, row in rows:
        where = f"ERROR: {label}:{line}: {CARD}"
        if len(row) != len(HEADER):
            errors.append(f"{where}: the row has {len(row)} cells, not {len(HEADER)}")
            continue
        parsed = parse_type(row[0])
        kind = SPAN.fullmatch(row[1])
        if parsed is None or kind is None:
            errors.append(f"{where}: the row {row!r} names no Parquet and column type")
            continue
        physical, names = parsed
        for name in names:
            table.setdefault((physical, name, kind.group(1)), line)
    profile = profile_pairs(document)
    for physical, name, kind in sorted(set(table) - profile):
        errors.append(
            f"ERROR: {label}:{table[physical, name, kind]}: {CARD} reads {physical} "
            f"annotated {name} as {kind}, which {PROFILE} does not"
        )
    for physical, name, kind in sorted(profile - set(table)):
        errors.append(
            f"ERROR: {PROFILE}: reads {physical} annotated {name} as {kind}, which "
            f"{label} {CARD} does not"
        )
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", type=Path, default=ROOT)
    errors = check(parser.parse_args().root)
    for error in errors:
        print(error)
    if errors:
        return 1
    print(f"PASS: {CARD}'s table reads what {PROFILE} reads.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
