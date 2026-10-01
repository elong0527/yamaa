# Reference solution for the yamaa benchmark sdtm-lb-multiform (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value) != ""


odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

# Each form occurrence keeps its own date; lesional comes before
# non-lesional from the same form.
test_meta = {
    "VITD": ("VITD25OH", "25-Hydroxyvitamin D", "CHEMISTRY", "SERUM", None, "ng/mL"),
    "IL13_LES": ("IL13", "Interleukin 13 mRNA", "GENE EXPRESSION", "SKIN BIOPSY", "LESIONAL", "CYCLE"),
    "IL13_NONLES": ("IL13", "Interleukin 13 mRNA", "GENE EXPRESSION", "SKIN BIOPSY", "NON-LESIONAL", "CYCLE"),
    "SAL_CAMP": ("CAMPPRO", "Cathelicidin Protein", "ANTIMICROBIAL PEPTIDE", "SALIVA", None, "ng/mL"),
    "TS_CAMP_LES": ("CAMPPRO", "Cathelicidin Protein", "ANTIMICROBIAL PEPTIDE", "TAPE STRIP", "LESIONAL", "ng/mL"),
    "TS_CAMP_NONLES": ("CAMPPRO", "Cathelicidin Protein", "ANTIMICROBIAL PEPTIDE", "TAPE STRIP", "NON-LESIONAL", "ng/mL"),
}
visit_meta = {
    "SCRN": ("SCREENING", "1"),
    "BL": ("BASELINE", "2"),
    "D21": ("DAY 21", "3"),
}


def _result_key(itemoid: str) -> str | None:
    short = itemoid.split(".")[-1]
    return short if short in test_meta else None


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

rows = []
for (studyid, subj, event_oid, _group_oid, group_rep), values in groups.items():
    event_short = event_oid.split(".")[-1]
    if event_short not in visit_meta and event_short != "UNSCH":
        continue
    # The collection date from the date item on the same form.
    lbdtc = None
    for oid, val in values.items():
        if oid.split(".")[-1].endswith("DTC") and _present(val):
            lbdtc = str(val).strip()
    if event_short == "UNSCH":
        visit, visitnum = "UNSCHEDULED", f"2.0{group_rep}"
    else:
        visit, visitnum = visit_meta[event_short]
    for oid, val in values.items():
        rkey = _result_key(oid)
        if rkey is None:
            continue
        text = str(val).strip() if _present(val) else ""
        # An item with no reported value produces no record.
        if text == "":
            continue
        testcd, test, cat, spec, loc, unit = test_meta[rkey]
        if text == "NOT DONE":
            rows.append(
                {
                    "DOMAIN": "LB",
                    "STUDYID": studyid,
                    "USUBJID": subj,
                    "VISIT": visit,
                    "VISITNUM": visitnum,
                    "LBTESTCD": testcd,
                    "LBTEST": test,
                    "LBCAT": cat,
                    "LBSPEC": spec,
                    "LBLOC": loc,
                    "LBORRES": None,
                    "LBORRESU": None,
                    "LBSTRESC": None,
                    "LBSTRESN": None,
                    "LBSTRESU": None,
                    "LBSTAT": "NOT DONE",
                    "LBDTC": lbdtc,
                }
            )
        else:
            rows.append(
                {
                    "DOMAIN": "LB",
                    "STUDYID": studyid,
                    "USUBJID": subj,
                    "VISIT": visit,
                    "VISITNUM": visitnum,
                    "LBTESTCD": testcd,
                    "LBTEST": test,
                    "LBCAT": cat,
                    "LBSPEC": spec,
                    "LBLOC": loc,
                    "LBORRES": text,
                    "LBORRESU": unit,
                    "LBSTRESC": text,
                    "LBSTRESN": text,
                    "LBSTRESU": unit,
                    "LBSTAT": None,
                    "LBDTC": lbdtc,
                }
            )

loc_rank = {"LESIONAL": 0, "NON-LESIONAL": 1, None: 2}
rows.sort(
    key=lambda r: (
        r["USUBJID"],
        r["LBDTC"] or "",
        r["LBTESTCD"],
        r["LBSPEC"],
        loc_rank[r["LBLOC"]],
    )
)
seq_by_subject: dict[str, int] = {}
for row in rows:
    seq_by_subject[row["USUBJID"]] = seq_by_subject.get(row["USUBJID"], 0) + 1
    row["LBSEQ"] = str(seq_by_subject[row["USUBJID"]])

lb = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "LBSEQ": pl.String,
            "VISIT": pl.String,
            "VISITNUM": pl.String,
            "LBTESTCD": pl.String,
            "LBTEST": pl.String,
            "LBCAT": pl.String,
            "LBSPEC": pl.String,
            "LBLOC": pl.String,
            "LBORRES": pl.String,
            "LBORRESU": pl.String,
            "LBSTRESC": pl.String,
            "LBSTRESN": pl.String,
            "LBSTRESU": pl.String,
            "LBSTAT": pl.String,
            "LBDTC": pl.String,
        },
    )
    .with_columns(
        pl.col("LBSEQ").cast(pl.Int64).alias("_seq"),
        pl.col("VISITNUM").cast(pl.Float64).alias("_visitnum"),
    )
    .sort(["USUBJID", "_seq"])
    .select(
        "DOMAIN", "STUDYID", "USUBJID", "LBSEQ", "VISIT", "VISITNUM",
        "LBTESTCD", "LBTEST", "LBCAT", "LBSPEC", "LBLOC", "LBORRES",
        "LBORRESU", "LBSTRESC", "LBSTRESN", "LBSTRESU", "LBSTAT", "LBDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
lb.write_csv("/app/output/lb.csv")
