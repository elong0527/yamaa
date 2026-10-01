# Reference solution for the yamaa benchmark sdtm-lb-last-observation-before-exposure (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)
dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
mapping = pl.read_csv("/app/input/lb_mapping.csv", infer_schema=False)

ref_of = {
    r["USUBJID"]: (r["RFSTDTC"] if r["RFSTDTC"] not in (None, "") else None)
    for r in dm.to_dicts()
}
map_of = {
    r["ItemOID"]: (r["LBTESTCD"], r["LBSPEC"]) for r in mapping.to_dicts()
}
study_of = {}
groups = {}
for r in odm.to_dicts():
    study_of[r["SubjectKey"]] = r["StudyOID"]
    key = (r["SubjectKey"], r["ItemGroupRepeatKey"])
    groups.setdefault(key, {})[r["ItemOID"]] = r["Value"]

rows = []
for (subj, repkey), items in groups.items():
    lbdtc = items.get("IT.LB.LBDTC")
    lbdtc = lbdtc if lbdtc not in (None, "") else None
    # The test item in this group, via the item dictionary.
    found = None
    for oid, (code, spec) in map_of.items():
        if oid in items:
            found = (oid, code, spec)
            break
    if found is None:
        continue
    oid, code, spec = found
    raw = items.get(oid)
    has_result = raw is not None and raw != ""
    lbstat = items.get("IT.LB.LBSTAT")
    rows.append(
        {
            "DOMAIN": "LB",
            "STUDYID": study_of[subj],
            "USUBJID": subj,
            "LBSEQ": int(repkey),
            "LBTESTCD": code,
            "LBSPEC": spec,
            "LBORRES": raw if has_result else None,
            "LBDTC": lbdtc,
            "LBSTAT": "NOT DONE" if not has_result else None,
            "_HAS": has_result,
        }
    )

# Latest record with a result on or before the reference start date,
# per subject, test, and specimen; ties go to the higher sequence number.
flag_keys = {}
for row in sorted(rows, key=lambda r: (r["LBDTC"] or "", r["LBSEQ"])):
    if not row["_HAS"] or row["LBDTC"] is None:
        continue
    ref = ref_of.get(row["USUBJID"])
    if ref is not None and row["LBDTC"] > ref:
        continue
    flag_keys[(row["USUBJID"], row["LBTESTCD"], row["LBSPEC"])] = row["LBSEQ"]

for row in rows:
    key = (row["USUBJID"], row["LBTESTCD"], row["LBSPEC"])
    row["LBLOBXFL"] = (
        "Y" if row["_HAS"] and flag_keys.get(key) == row["LBSEQ"] else None
    )
    del row["_HAS"]

lb = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "LBSEQ": pl.Int64,
            "LBTESTCD": pl.String,
            "LBSPEC": pl.String,
            "LBORRES": pl.String,
            "LBDTC": pl.String,
            "LBSTAT": pl.String,
            "LBLOBXFL": pl.String,
        },
    )
    .sort(["STUDYID", "USUBJID", "LBSEQ"])
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "LBSPEC",
        "LBORRES",
        "LBDTC",
        "LBSTAT",
        "LBLOBXFL",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
lb.write_csv("/app/output/lb.csv")
