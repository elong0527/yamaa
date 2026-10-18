# Reference solution for the yamaa benchmark adam-adae-serious-sequence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")


def main():
    with open(INPUT_DIR / "ae.csv", newline="") as f:
        rows = list(csv.DictReader(f))

    # ASTDT is the start date collected in AESTDTC, blank when none.
    for row in rows:
        row["ASTDT"] = (row.get("AESTDTC") or "").strip()
        row["SERSEQ"] = ""

    # SERSEQ numbers each subject's serious events (AESER "Y") from 1,
    # ordered by ASTDT with AESEQ breaking ties; serious events with no
    # ASTDT come after every dated one, in AESEQ order among themselves.
    serious = {}
    for idx, row in enumerate(rows):
        if (row.get("AESER") or "").strip() == "Y":
            serious.setdefault(row["USUBJID"], []).append(idx)

    for idxs in serious.values():
        def sort_key(i):
            r = rows[i]
            return (0 if r["ASTDT"] else 1, r["ASTDT"], int(r["AESEQ"]))

        for n, i in enumerate(sorted(idxs, key=sort_key), start=1):
            rows[i]["SERSEQ"] = str(n)

    fieldnames = ["STUDYID", "USUBJID", "AESEQ", "AESER", "ASTDT", "SERSEQ"]
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_DIR / "adae.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow({k: row.get(k, "") for k in fieldnames})


if __name__ == "__main__":
    main()
