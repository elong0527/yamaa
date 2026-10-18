# Reference solution for the yamaa benchmark adam-adsl-flag-chain (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import defaultdict

INPUT = "/app/input"
OUTPUT = "/app/output"

with open(f"{INPUT}/dm.csv", newline="") as f:
    dm = list(csv.DictReader(f))

with open(f"{INPUT}/ex.csv", newline="") as f:
    ex = list(csv.DictReader(f))

# Earliest dated exposure start per subject
trtsdt = {}
for row in ex:
    dtc = (row.get("EXSTDTC") or "").strip()
    if not dtc:
        continue
    subj = row["USUBJID"]
    if subj not in trtsdt or dtc < trtsdt[subj]:
        trtsdt[subj] = dtc

rows = []
for subj in dm:
    usubjid = subj["USUBJID"]
    randdt = (subj.get("RANDDT") or "").strip()
    age = int(subj["AGE"])
    saffl = "Y" if usubjid in trtsdt else "N"
    ittfl = "Y" if randdt else "N"
    popfl = "Y" if (saffl == "Y" and ittfl == "Y") else "N"
    rows.append({
        "POPFL": popfl,
        "SAFFL": saffl,
        "ITTFL": ittfl,
        "TRTSDT": trtsdt.get(usubjid, ""),
        "RANDDT": randdt,
        "AGEGR1": "<65" if age < 65 else ">=65",
        "STUDYID": subj["STUDYID"],
        "USUBJID": usubjid,
        "AGE": subj["AGE"],
        "_age": age,
    })

# Rank by AGE within study, ties broken by USUBJID
by_study = defaultdict(list)
for r in rows:
    by_study[r["STUDYID"]].append(r)
for study_rows in by_study.values():
    for rank, r in enumerate(sorted(study_rows, key=lambda x: (x["_age"], x["USUBJID"])), start=1):
        r["AGERNK"] = rank

fieldnames = ["POPFL", "SAFFL", "ITTFL", "TRTSDT", "RANDDT", "AGEGR1",
              "AGERNK", "STUDYID", "USUBJID", "AGE"]
with open(f"{OUTPUT}/adsl.csv", "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=fieldnames)
    w.writeheader()
    for r in rows:
        w.writerow({k: r[k] for k in fieldnames})
