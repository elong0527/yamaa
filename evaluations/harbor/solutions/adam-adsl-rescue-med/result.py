# Reference solution for the yamaa benchmark adam-adsl-rescue-med (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")


def read_rows(name):
    with open(INPUT / name, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def main():
    dm = read_rows("dm.csv")
    cm = read_rows("cm.csv")

    rescue = {}
    for row in cm:
        if (row.get("CMCAT") or "") != "RESCUE MEDICATION":
            continue
        key = (row.get("STUDYID") or "", row.get("USUBJID") or "")
        # Compare the start date as written: a year-month value such as
        # "2025-03" sorts before any full date in the same month under
        # plain string ordering. A missing date sorts as the empty string.
        date_key = row.get("CMSTDTC") or ""
        try:
            seq_key = int(row.get("CMSEQ") or "")
        except ValueError:
            try:
                seq_key = float(row.get("CMSEQ") or "")
            except ValueError:
                seq_key = 0
        sort_key = (date_key, seq_key)
        if key not in rescue or sort_key < rescue[key][0]:
            rescue[key] = (sort_key, row.get("CMTRT") or "")

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT / "adsl.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(["STUDYID", "USUBJID", "RESCTRT"])
        for row in dm:
            studyid = row.get("STUDYID") or ""
            usubjid = row.get("USUBJID") or ""
            entry = rescue.get((studyid, usubjid))
            resctrt = entry[1] if entry else ""
            writer.writerow([studyid, usubjid, resctrt])


if __name__ == "__main__":
    main()
