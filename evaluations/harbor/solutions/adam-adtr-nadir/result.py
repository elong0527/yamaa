# Reference solution for the yamaa benchmark adam-adtr-nadir (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from datetime import date

INPUT = "/app/input/adtr_raw.csv"
OUTPUT = "/app/output/adtr.csv"

COLUMNS = [
    "STUDYID", "USUBJID", "AVISIT", "AVISITN", "ADT", "PARAMCD", "PARAM",
    "AVAL", "NMEAS", "NTARGET", "ANL01FL", "NADIR",
]


def parse_date(text):
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text)


def fmt_num(value):
    if value is None:
        return ""
    if float(value).is_integer():
        return str(int(value))
    return str(value)


with open(INPUT, newline="") as f:
    rows = list(csv.DictReader(f))

# Complete assessments per subject: ANL01FL "Y" with a dated ADT.
complete = {}
for row in rows:
    if (row.get("ANL01FL") or "").strip() == "Y":
        adt = parse_date(row.get("ADT"))
        aval_text = (row.get("AVAL") or "").strip()
        if adt is not None and aval_text:
            complete.setdefault(row["USUBJID"], []).append((adt, float(aval_text)))

for row in rows:
    adt = parse_date(row.get("ADT"))
    nadir = None
    if adt is not None:
        candidates = [aval for d, aval in complete.get(row["USUBJID"], [])
                      if d <= adt]
        if candidates:
            nadir = min(candidates)
    row["NADIR"] = fmt_num(nadir)

with open(OUTPUT, "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=COLUMNS)
    writer.writeheader()
    for row in rows:
        writer.writerow({c: row.get(c, "") for c in COLUMNS})
