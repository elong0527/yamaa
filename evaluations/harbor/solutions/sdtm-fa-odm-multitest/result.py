# Reference solution for the yamaa benchmark sdtm-fa-odm-multitest (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

forms = {}
study_of = {}
for r in odm.to_dicts():
    study_of[r["SubjectKey"]] = r["StudyOID"]
    if r["ItemGroupOID"] != "IG.FA":
        continue
    key = (
        r["SubjectKey"],
        r["StudyEventOID"],
        r["StudyEventRepeatKey"],
        r["ItemGroupRepeatKey"],
    )
    forms.setdefault(key, {})[r["ItemOID"]] = r["Value"]

day_order = {"DAY1": 1, "DAY2": 2}
tpt_of = {"DAY1": "END DAY 1", "DAY2": "END DAY 2"}
test_order = {"IT.FA.OCCUR": 0, "IT.FA.SEV": 1, "IT.FA.LDIAM": 2}
test_info = {
    "IT.FA.OCCUR": ("OCCUR", "Occurrence Indicator"),
    "IT.FA.SEV": ("SEV", "Severity/Intensity"),
    "IT.FA.LDIAM": ("LDIAM", "Longest Diameter"),
}

rows = []
for (subj, event, _rep, repkey), form in forms.items():
    obj = form.get("IT.FA.FAOBJ")
    fadtc = form.get("IT.FA.FADTC")
    if obj is None or obj == "":
        continue
    for oid in ("IT.FA.OCCUR", "IT.FA.SEV", "IT.FA.LDIAM"):
        if oid not in form:
            continue
        raw = form[oid]
        blank = raw is None or raw == ""
        val = None if blank else raw
        code, name = test_info[oid]
        rows.append(
            {
                "DOMAIN": "FA",
                "STUDYID": study_of[subj],
                "USUBJID": subj,
                "DAY": day_order.get(event, 99),
                "REP": int(repkey),
                "ORDER": test_order[oid],
                "FATESTCD": code,
                "FATEST": name,
                "FAOBJ": obj,
                "FACAT": "REACTOGENICITY",
                "FAORRES": val,
                "FAORRESU": "mm" if oid == "IT.FA.LDIAM" else None,
                "FASTRESC": val,
                "FASTAT": "NOT DONE" if blank else None,
                "FATPT": tpt_of.get(event),
                "FADTC": fadtc,
            }
        )

fa = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "DAY": pl.Int64,
            "REP": pl.Int64,
            "ORDER": pl.Int64,
            "FATESTCD": pl.String,
            "FATEST": pl.String,
            "FAOBJ": pl.String,
            "FACAT": pl.String,
            "FAORRES": pl.String,
            "FAORRESU": pl.String,
            "FASTRESC": pl.String,
            "FASTAT": pl.String,
            "FATPT": pl.String,
            "FADTC": pl.String,
        },
    )
    .sort(["STUDYID", "USUBJID", "DAY", "REP", "ORDER"])
    .with_columns(
        pl.int_range(1, pl.len() + 1).over(["STUDYID", "USUBJID"]).alias("FASEQ")
    )
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "FASEQ",
        "FATESTCD",
        "FATEST",
        "FAOBJ",
        "FACAT",
        "FAORRES",
        "FAORRESU",
        "FASTRESC",
        "FASTAT",
        "FATPT",
        "FADTC",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
fa.write_csv("/app/output/fa.csv")
