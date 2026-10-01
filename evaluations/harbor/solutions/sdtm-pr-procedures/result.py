# Reference solution for the yamaa benchmark sdtm-pr-procedures (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value) != ""


odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

# One record per reported procedure and per answer to the pre-specified
# radiotherapy question: each form occurrence is a record.
groups: dict[tuple, dict] = {}
for row in odm.to_dicts():
    key = (
        str(row["StudyOID"]),
        str(row["SubjectKey"]),
        str(row["StudyEventOID"]),
        str(row["ItemGroupOID"]),
        str(row["ItemGroupRepeatKey"]),
    )
    groups.setdefault(key, {})[str(row["ItemOID"])] = row["Value"]

cat_by_group = {
    "IG.PR.SURGERY": "PRIOR CANCER SURGERY",
    "IG.PR.RADIOTHERAPY": "PRIOR RADIOTHERAPY",
}

rows = []
for (studyid, subjkey, _event, group_oid, _rep), values in groups.items():
    # USUBJID carries the study prefix when the ODM subject key does not.
    usubjid = subjkey if subjkey.startswith(studyid + "-") else f"{studyid}-{subjkey}"
    is_radio = group_oid == "IG.PR.RADIOTHERAPY"
    occurs = str(values.get("IT.PR.RTPRSP") or "") if is_radio else ""
    prtrt = str(values.get("IT.PR.PRTRT") or "")
    if is_radio:
        prpresp, procur = "Y", occurs if _present(occurs) else None
    else:
        prpresp, procur = None, None
    # Dose only when the procedure occurred; a "No" keeps no dose or dates.
    if is_radio and procur == "Y":
        dose = str(values.get("IT.PR.PRDOSE") or "")
        dose = dose if _present(dose) else None
        dosu = str(values.get("IT.PR.PRDOSU") or "")
        dosu = dosu if _present(dosu) else None
        stdtc = str(values.get("IT.PR.PRSTDTC") or "")
        stdtc = stdtc if _present(stdtc) else None
        endtc = str(values.get("IT.PR.PRENDTC") or "")
        endtc = endtc if _present(endtc) else None
    elif not is_radio:
        dose = dosu = None
        stdtc = str(values.get("IT.PR.PRSTDTC") or "")
        stdtc = stdtc if _present(stdtc) else None
        endtc = None
        # Surgery has no end date in the form.
        if "IT.PR.PRENDTC" in values and _present(values.get("IT.PR.PRENDTC")):
            endtc = str(values.get("IT.PR.PRENDTC"))
    else:
        dose = dosu = stdtc = endtc = None
    rows.append(
        {
            "DOMAIN": "PR",
            "STUDYID": studyid,
            "USUBJID": usubjid,
            "PRTRT": prtrt,
            "PRCAT": cat_by_group[group_oid],
            "PRPRESP": prpresp,
            "PROCCUR": procur,
            "PRLOC": str(values.get("IT.PR.PRLOC") or "") or None,
            "PRLAT": str(values.get("IT.PR.PRLAT") or "") or None,
            "PRDOSE": dose,
            "PRDOSU": dosu,
            "PRSTDTC": stdtc,
            "PRENDTC": endtc,
        }
    )

# PRSEQ numbers each subject's records by start date, then procedure name.
rows.sort(key=lambda r: (r["USUBJID"], r["PRSTDTC"] is None, r["PRSTDTC"] or "", r["PRTRT"]))
seq_by_subject: dict[str, int] = {}
for row in rows:
    seq_by_subject[row["USUBJID"]] = seq_by_subject.get(row["USUBJID"], 0) + 1
    row["PRSEQ"] = str(seq_by_subject[row["USUBJID"]])

pr = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "PRSEQ": pl.String,
            "PRTRT": pl.String,
            "PRCAT": pl.String,
            "PRPRESP": pl.String,
            "PROCCUR": pl.String,
            "PRLOC": pl.String,
            "PRLAT": pl.String,
            "PRDOSE": pl.String,
            "PRDOSU": pl.String,
            "PRSTDTC": pl.String,
            "PRENDTC": pl.String,
        },
    )
    .with_columns(pl.col("PRSEQ").cast(pl.Int64).alias("_seq"))
    .sort(["USUBJID", "_seq"])
    .select(
        "DOMAIN", "STUDYID", "USUBJID", "PRSEQ", "PRTRT", "PRCAT", "PRPRESP",
        "PROCCUR", "PRLOC", "PRLAT", "PRDOSE", "PRDOSU", "PRSTDTC", "PRENDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
pr.write_csv("/app/output/pr.csv")
