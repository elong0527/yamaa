# Reference solution for the yamaa benchmark adam-adrs-best-response (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

AVAL_BY_AVALC = {
    "CR": "1",
    "PR": "2",
    "SD": "3",
    "NON-CR/NON-PD": "4",
    "PD": "5",
    "NE": "6",
}

with open("/app/input/adsl.csv", newline="") as f:
    adsl = list(csv.DictReader(f))

with open("/app/input/adrs_selection.csv", newline="") as f:
    selection = list(csv.DictReader(f))

best = {}
for row in selection:
    if row["BORSEQ"] == "1":
        best[row["USUBJID"]] = row

out = []
for subj in adsl:
    usubjid = subj["USUBJID"]
    sel = best.get(usubjid)
    avalc = sel["BORCAT"] if sel else ""
    aval = AVAL_BY_AVALC[avalc] if avalc else ""
    adt = sel["ADT"] if sel else ""
    out.append(
        {
            "STUDYID": subj["STUDYID"],
            "USUBJID": usubjid,
            "PARAMCD": "BOR",
            "PARAM": "Best Overall Response by Investigator",
            "RANDDT": subj["RANDDT"],
            "AVALC": avalc,
            "AVAL": aval,
            "ADT": adt,
        }
    )

fields = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "RANDDT", "AVALC", "AVAL", "ADT"]
with open("/app/output/adrs.csv", "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=fields)
    writer.writeheader()
    writer.writerows(out)
