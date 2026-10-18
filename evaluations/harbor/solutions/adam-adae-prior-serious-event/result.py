# Reference solution for the yamaa benchmark adam-adae-prior-serious-event (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

IN = "/app/input/ae.csv"
OUT = "/app/output/adae.csv"

with open(IN, newline="") as f:
    rows = list(csv.DictReader(f))

for r in rows:
    r["AESEQ"] = int(r["AESEQ"])
    if r["AEDECOD"] == "":
        r["AEDECOD"] = None

rows.sort(key=lambda r: (r["USUBJID"], r["AESEQ"]))

out = []
by_subj = {}
for r in rows:
    by_subj.setdefault(r["USUBJID"], []).append(r)

for subj, evs in by_subj.items():
    serious = [e for e in evs if e["AESER"] == "Y"]
    first = serious[0] if serious else None
    prev_decod = None
    for i, e in enumerate(evs):
        row = {
            "STUDYID": e["STUDYID"],
            "USUBJID": e["USUBJID"],
            "AESEQ": e["AESEQ"],
            "AEDECOD": e["AEDECOD"] or "",
            "AESER": e["AESER"],
            "PRIOR_SAEFL": "",
            "PRIOR_SAEDECOD": "",
            "PRIOR_SAESEQ": "",
            "FIRST_SAEDECOD": "",
            "FIRST_SAESEQ": "",
            "PREV_AEDECOD": prev_decod or "",
        }
        if e["AESER"] != "Y":
            earlier = [s for s in serious if s["AESEQ"] < e["AESEQ"]]
            if earlier:
                most_recent = earlier[-1]
                row["PRIOR_SAEFL"] = "Y"
                row["PRIOR_SAEDECOD"] = most_recent["AEDECOD"] or ""
                row["PRIOR_SAESEQ"] = most_recent["AESEQ"]
            if first is not None:
                row["FIRST_SAEDECOD"] = first["AEDECOD"] or ""
                row["FIRST_SAESEQ"] = first["AESEQ"]
        out.append(row)
        prev_decod = e["AEDECOD"]

cols = ["STUDYID", "USUBJID", "AESEQ", "AEDECOD", "AESER", "PRIOR_SAEFL",
        "PRIOR_SAEDECOD", "PRIOR_SAESEQ", "FIRST_SAEDECOD", "FIRST_SAESEQ",
        "PREV_AEDECOD"]
with open(OUT, "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=cols)
    w.writeheader()
    w.writerows(out)
