# Reference solution for the yamaa benchmark adam-adex-dose-reduction (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADEX from EX: flag administrations whose dose is lower than the
dose of the subject's immediately preceding administration."""

import csv

INPUT = "/app/input/ex.csv"
OUTPUT = "/app/output/adex.csv"

COLUMNS = ["STUDYID", "USUBJID", "EXSEQ", "EXSTDTM", "EXDOSE", "DOSREDFL"]


def parse_dose(text):
    """Return the numeric dose, or None when the value is missing."""
    if text is None or text.strip() == "":
        return None
    return float(text)


def main():
    with open(INPUT, newline="") as f:
        rows = list(csv.DictReader(f))

    # Order administrations within each subject by start date/time,
    # breaking ties by sequence number.
    ordered = sorted(
        rows,
        key=lambda r: (r["USUBJID"], r["EXSTDTM"], int(r["EXSEQ"])),
    )

    previous = {}  # subject -> numeric dose of the preceding administration
    for row in ordered:
        subject = row["USUBJID"]
        current = parse_dose(row["EXDOSE"])
        flag = ""
        if subject in previous:
            prev = previous[subject]
            if (
                current is not None
                and prev is not None
                and current != 0
                and prev != 0
                and current < prev
            ):
                flag = "Y"
        row["DOSREDFL"] = flag
        previous[subject] = current

    with open(OUTPUT, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        for row in ordered:
            writer.writerow({col: row[col] for col in COLUMNS})


if __name__ == "__main__":
    main()
