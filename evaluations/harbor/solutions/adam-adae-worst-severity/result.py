# Reference solution for the yamaa benchmark adam-adae-worst-severity (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADAE dataset from ADAE_RAW.

One record per adverse event, with the numeric severity rank ASEVN and
AOCCPIFL flagging, per subject and preferred term, the first occurrence
of the worst severity among treatment-emergent, severity-graded events.
"""

import csv
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")

SEVERITY_RANK = {"MILD": 1, "MODERATE": 2, "SEVERE": 3}

OUTPUT_COLUMNS = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AEBODSYS",
    "AEDECOD",
    "ASTDT",
    "AESEV",
    "TRTEMFL",
    "ASEVN",
    "AOCCPIFL",
]


def severity_rank(value):
    """Numeric rank of an AESEV value; None when AESEV has no value."""
    if value is None:
        return None
    return SEVERITY_RANK.get(value.strip())


def sequence_key(value):
    """Sort key for AESEQ so the lowest sequence number wins a tie."""
    try:
        return (0, int(value), "")
    except (TypeError, ValueError):
        return (1, 0, "" if value is None else str(value))


def occurrence_key(record):
    """Rank eligible events: worst severity, then earliest start date,
    then lowest AESEQ. A missing start date never wins a date tie."""
    astdt = (record.get("ASTDT") or "").strip()
    return (
        -record["ASEVN"],
        1 if not astdt else 0,
        astdt,
        sequence_key(record.get("AESEQ")),
    )


def main():
    with open(INPUT_DIR / "adae_raw.csv", newline="", encoding="utf-8") as handle:
        records = list(csv.DictReader(handle))

    for record in records:
        record["ASEVN"] = severity_rank(record.get("AESEV"))
        record["AOCCPIFL"] = ""

    groups = {}
    for index, record in enumerate(records):
        eligible = (
            (record.get("TRTEMFL") or "").strip() == "Y"
            and record["ASEVN"] is not None
        )
        if eligible:
            key = (record.get("USUBJID"), record.get("AEDECOD"))
            groups.setdefault(key, []).append(index)

    for indexes in groups.values():
        winner = min(indexes, key=lambda i: occurrence_key(records[i]))
        records[winner]["AOCCPIFL"] = "Y"

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_DIR / "adae.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=OUTPUT_COLUMNS)
        writer.writeheader()
        for record in records:
            row = {column: record.get(column) or "" for column in OUTPUT_COLUMNS}
            row["ASEVN"] = "" if record["ASEVN"] is None else str(record["ASEVN"])
            writer.writerow(row)


if __name__ == "__main__":
    main()
