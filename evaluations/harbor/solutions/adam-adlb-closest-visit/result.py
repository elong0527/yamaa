# Reference solution for the yamaa benchmark adam-adlb-closest-visit (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT = "/app/input/adlb_raw.csv"
OUTPUT = "/app/output/adlb.csv"

with open(INPUT, newline="") as f:
    rows = list(csv.DictReader(f))

for row in rows:
    ady_raw = (row["ADY"] or "").strip()
    ady = int(ady_raw) if ady_raw else None
    row["_ady"] = ady
    if ady is not None and 8 <= ady <= 22:
        row["AVISIT"] = "WEEK 2"
        row["AWTARGET"] = 15
        row["ADIST"] = abs(ady - 15)
    else:
        row["AVISIT"] = ""
        row["AWTARGET"] = ""
        row["ADIST"] = ""
    row["ANL01FL"] = ""

groups = {}
for i, row in enumerate(rows):
    if row["AVISIT"]:
        key = (row["USUBJID"], row["PARAMCD"], row["AVISIT"])
        groups.setdefault(key, []).append(i)

for idxs in groups.values():
    best = min(
        idxs,
        key=lambda i: (rows[i]["ADIST"], -rows[i]["_ady"], int(rows[i]["LBSEQ"])),
    )
    rows[best]["ANL01FL"] = "Y"

columns = [
    "STUDYID", "USUBJID", "PARAMCD", "LBSEQ", "ADT", "ADY", "AVAL",
    "AVISIT", "AWTARGET", "ADIST", "ANL01FL",
]

with open(OUTPUT, "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=columns, extrasaction="ignore")
    writer.writeheader()
    for row in rows:
        writer.writerow(row)
