# Reference solution for the yamaa benchmark adam-adsl-rescue-med (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from datetime import date
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")


def read_rows(name):
    with open(INPUT / name, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def complete(text):
    """A start date completed to its first possible day, with its flag."""
    parts = text.split("-")
    if len(parts) == 1:
        return date(int(parts[0]), 1, 1), "M"
    if len(parts) == 2:
        return date(int(parts[0]), int(parts[1]), 1), "D"
    return date.fromisoformat(text), ""


def main():
    dm = read_rows("dm.csv")
    cm = read_rows("cm.csv")

    rescue = {}
    for row in cm:
        if row["CMCAT"] != "RESCUE MEDICATION":
            continue
        key = (row["STUDYID"], row["USUBJID"])
        start, flag = complete(row["CMSTDTC"]) if row["CMSTDTC"] else (None, "")
        # A dated record sorts before an undated one, then by date and
        # sequence number.
        sort_key = (start is None, start or date.min, int(row["CMSEQ"]))
        if key not in rescue or sort_key < rescue[key][0]:
            rescue[key] = (sort_key, start, flag, row["CMDECOD"])

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT / "adsl.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(
            ["STUDYID", "USUBJID", "RESCFL", "RESCDT", "RESCDTF", "RESCTRT"]
        )
        for row in dm:
            entry = rescue.get((row["STUDYID"], row["USUBJID"]))
            if entry is None:
                writer.writerow([row["STUDYID"], row["USUBJID"], "N", "", "", ""])
                continue
            _, start, flag, name = entry
            writer.writerow(
                [
                    row["STUDYID"],
                    row["USUBJID"],
                    "Y",
                    start.isoformat() if start else "",
                    flag,
                    name,
                ]
            )


if __name__ == "__main__":
    main()
