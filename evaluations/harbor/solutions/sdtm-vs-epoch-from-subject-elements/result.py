# Reference solution for the yamaa benchmark sdtm-vs-epoch-from-subject-elements (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM VS dataset, deriving EPOCH from the subject's SE elements.

Reads SE and VS_RAW from /app/input and writes /app/output/vs.csv with one
record per vital signs measurement.

EPOCH is the trial period of the subject's element in progress at the
collection date-time: the element whose start is on or before the collection
and whose end is on or after it. A date-only collection is compared by date,
with no time assumed: the element whose start date is on or before it and
whose end date is on or after it. A reading that falls in two elements
belongs to the later one. EPOCH is blank when the reading has no date or
only a partial one, or when it falls outside every element.
"""

import csv
import re
from datetime import date, datetime

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

FULL_DATETIME = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?$")
FULL_DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


def parse_element_bound(text):
    """Parse an SE start/end date-time (always fully specified)."""
    return datetime.fromisoformat(text)


def parse_collection(text):
    """Return the collection date-time or date, or None if missing or partial.

    A full date-time parses to a datetime; a full date alone parses to a
    date, with no time assumed. Anything else (empty, year-month, year
    only) yields None.
    """
    if not text:
        return None
    text = text.strip()
    if FULL_DATETIME.match(text):
        return datetime.fromisoformat(text)
    if FULL_DATE.match(text):
        return date.fromisoformat(text)
    return None


def covers(element, collection):
    """Whether the element is in progress at the collection: by moment for a
    date-time, by calendar date for a date alone."""
    if isinstance(collection, datetime):
        return element["start"] <= collection <= element["end"]
    return element["start"].date() <= collection <= element["end"].date()


def derive_epoch(collection, elements):
    """Return the EPOCH of the element in progress at the collection.

    When two elements cover the collection, at a shared boundary instant
    or on a shared boundary date, the later element (the one starting at
    the boundary) wins.
    """
    if collection is None:
        return ""
    matches = [element for element in elements if covers(element, collection)]
    if not matches:
        return ""
    later = max(matches, key=lambda element: element["start"])
    return later["epoch"]


def main():
    with open(f"{INPUT_DIR}/se.csv", newline="") as handle:
        se_rows = list(csv.DictReader(handle))

    elements_by_subject = {}
    for row in se_rows:
        key = (row["STUDYID"], row["USUBJID"])
        elements_by_subject.setdefault(key, []).append(
            {
                "epoch": row["EPOCH"],
                "start": parse_element_bound(row["SESTDTC"]),
                "end": parse_element_bound(row["SEENDTC"]),
            }
        )

    with open(f"{INPUT_DIR}/vs_raw.csv", newline="") as handle:
        vs_rows = list(csv.DictReader(handle))

    out_rows = []
    for row in vs_rows:
        key = (row["STUDYID"], row["USUBJID"])
        elements = elements_by_subject.get(key, [])
        collection = parse_collection(row["VSDTC"])
        out_rows.append(
            {
                "DOMAIN": "VS",
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "VSSEQ": row["VSSEQ"],
                "VSTESTCD": row["VSTESTCD"],
                "VSORRES": row["VSORRES"],
                "VSDTC": row["VSDTC"],
                "EPOCH": derive_epoch(collection, elements),
            }
        )

    fieldnames = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "VSSEQ",
        "VSTESTCD",
        "VSORRES",
        "VSDTC",
        "EPOCH",
    ]
    with open(f"{OUTPUT_DIR}/vs.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
