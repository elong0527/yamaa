# Reference solution for the yamaa benchmark adam-adae-phase-emergence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Derive ADAE with phase-specific treatment-emergent flags from ADSL and AE."""

from datetime import date, timedelta
from pathlib import Path

import polars as pl

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

PHASES = [("TR01SDT", "TR01EDT"), ("TR02SDT", "TR02EDT"), ("TR03SDT", "TR03EDT")]
FLAGS = ["TREM01FL", "TREM02FL", "TREM03FL"]
COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "AETERM", "AESER", "ASTDT",
           "TREM01FL", "TREM02FL", "TREM03FL", "TRTEMFL"]
WINDOW = {"N": 30, "Y": 90}


def to_date(value):
    """Return a date for a complete ISO 8601 date, otherwise None."""
    if value is None:
        return None
    text = str(value).strip()
    if len(text) < 10:
        return None
    try:
        return date.fromisoformat(text[:10])
    except ValueError:
        return None


def phase_windows(subject, aeser):
    """List (phase index, start, end) for the phases a subject entered; end None = open."""
    entered = []
    for index, (start_col, end_col) in enumerate(PHASES):
        start = to_date(subject.get(start_col))
        if start is not None:
            entered.append((index, start, to_date(subject.get(end_col))))
    windows = []
    for position, (index, start, last) in enumerate(entered):
        if position + 1 < len(entered):
            end = entered[position + 1][1] - timedelta(days=1)
        elif str(subject.get("EOTSTT") or "").strip().upper() == "ONGOING":
            end = None
        elif last is None:
            end = None
        else:
            end = last + timedelta(days=WINDOW.get(aeser, 30))
        windows.append((index, start, end))
    return windows


def derive_flags(subject, astdt, aeser):
    flags = [None, None, None]
    if subject is None or astdt is None:
        return flags
    if to_date(subject.get("TR01SDT")) is None:
        return flags
    for index, start, end in phase_windows(subject, aeser):
        if astdt >= start and (end is None or astdt <= end):
            flags[index] = "Y"
            break
    return flags


def main():
    # Every column is read as text; an empty field is null.
    adsl = pl.read_csv(INPUT / "adsl.csv", infer_schema=False)
    ae = pl.read_csv(INPUT / "ae.csv", infer_schema=False)
    subjects = {row["USUBJID"]: row for row in adsl.to_dicts()}

    records = []
    for row in ae.to_dicts():
        astdt = to_date(row.get("AESTDTC"))
        aeser = (row.get("AESER") or "").strip().upper()
        flags = derive_flags(subjects.get(row["USUBJID"]), astdt, aeser)
        records.append({
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "AESEQ": int(row["AESEQ"]),
            "AETERM": row["AETERM"],
            "AESER": row["AESER"],
            "ASTDT": astdt.isoformat() if astdt else None,
            "TREM01FL": flags[0],
            "TREM02FL": flags[1],
            "TREM03FL": flags[2],
            "TRTEMFL": "Y" if "Y" in flags else None,
        })

    schema = {name: pl.Utf8 for name in COLUMNS} | {"AESEQ": pl.Int64}
    adae = pl.DataFrame(records, schema=schema).sort(
        ["STUDYID", "USUBJID", "AESEQ"], maintain_order=True
    )
    OUTPUT.mkdir(parents=True, exist_ok=True)
    adae.write_csv(OUTPUT / "adae.csv")


if __name__ == "__main__":
    main()
