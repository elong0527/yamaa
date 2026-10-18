# Reference solution for the yamaa benchmark adam-adae-text-cleanup (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import re

INPUT = "/app/input/ae.csv"
OUTPUT = "/app/output/adae.csv"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AESPID",
    "AEREFNUM",
    "AETERM",
    "AETERMLO",
    "AERELLC",
    "AREL",
]

REFNUM_PATTERN = re.compile(r"^AE-(\d{3})$")


def aerefnum(aespid):
    if aespid is None or aespid == "":
        return 0
    match = REFNUM_PATTERN.match(aespid)
    if match:
        return int(match.group(1))
    return -1


def main():
    with open(INPUT, newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    out_rows = []
    for row in rows:
        aespid = row.get("AESPID") or ""
        aeterm = row.get("AETERM") or ""
        aerel = row.get("AEREL") or ""
        aerellc = aerel.lower() if aerel != "" else "not reported"
        out_rows.append(
            {
                "STUDYID": row.get("STUDYID") or "",
                "USUBJID": row.get("USUBJID") or "",
                "AESEQ": row.get("AESEQ") or "",
                "AESPID": aespid,
                "AEREFNUM": aerefnum(aespid),
                "AETERM": aeterm,
                "AETERMLO": aeterm.lower(),
                "AERELLC": aerellc,
                "AREL": aerellc.upper(),
            }
        )

    with open(OUTPUT, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
