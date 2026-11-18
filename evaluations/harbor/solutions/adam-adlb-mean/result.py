# Reference solution for the yamaa benchmark adam-adlb-mean (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADLB from the collected LB dataset.

Every collected record is kept, and each subject and parameter with a
non-missing result gets one average record (DTYPE "AVERAGE") whose AVAL is
the mean of those results. ASEQ numbers each subject's records by
parameter, collected records in LBSEQ order and then the average.
"""

import csv
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")


def format_number(value):
    """Shortest text of a float, without a trailing .0."""
    text = repr(value)
    return text[:-2] if text.endswith(".0") else text


def main():
    with open(INPUT_DIR / "lb.csv", newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    records = []
    values = {}
    for row in rows:
        text = (row["LBSTRESN"] or "").strip()
        value = float(text) if text else None
        key = (row["STUDYID"], row["USUBJID"], row["LBTESTCD"])
        values.setdefault(key, [])
        if value is not None:
            values[key].append(value)
        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "LBSEQ": int(row["LBSEQ"]),
                "PARAMCD": row["LBTESTCD"],
                "AVAL": "" if value is None else format_number(value),
                "DTYPE": "",
            }
        )
    for (studyid, usubjid, paramcd), found in values.items():
        if found:
            records.append(
                {
                    "STUDYID": studyid,
                    "USUBJID": usubjid,
                    "LBSEQ": None,
                    "PARAMCD": paramcd,
                    "AVAL": format_number(sum(found) / len(found)),
                    "DTYPE": "AVERAGE",
                }
            )

    # Collected records in LBSEQ order, then the average, within parameter.
    records.sort(
        key=lambda r: (
            r["STUDYID"],
            r["USUBJID"],
            r["PARAMCD"],
            r["DTYPE"] == "AVERAGE",
            r["LBSEQ"] or 0,
        )
    )
    sequence = {}
    for record in records:
        subject = (record["STUDYID"], record["USUBJID"])
        sequence[subject] = sequence.get(subject, 0) + 1
        record["ASEQ"] = sequence[subject]
    records.sort(key=lambda r: (r["USUBJID"], r["ASEQ"]))

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    fieldnames = ["STUDYID", "USUBJID", "ASEQ", "LBSEQ", "PARAMCD", "AVAL", "DTYPE"]
    with open(OUTPUT_DIR / "adlb.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for record in records:
            record["LBSEQ"] = "" if record["LBSEQ"] is None else record["LBSEQ"]
            writer.writerow(record)


if __name__ == "__main__":
    main()
