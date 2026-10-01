# Reference solution for the yamaa benchmark sdtm-lb-lab-specific-ranges (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import date
from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)
lbrange = pl.read_csv("/app/input/lbrange.csv", infer_schema=False).with_columns(
    pl.col("AGELO").cast(pl.Int64),
    pl.col("AGEHI").cast(pl.Int64),
    pl.col("NRLO").cast(pl.Float64, strict=False),
    pl.col("NRHI").cast(pl.Float64, strict=False),
)

subjects = {}
labs = {}
study_of = {}
for r in odm.to_dicts():
    study_of[r["SubjectKey"]] = r["StudyOID"]
    if r["ItemGroupOID"] == "IG.DM":
        subjects.setdefault(r["SubjectKey"], {})[r["ItemOID"]] = (
            r["Value"] if r["Value"] not in (None, "") else None
        )
    elif r["ItemGroupOID"] == "IG.LB":
        key = (r["SubjectKey"], r["ItemGroupRepeatKey"])
        labs.setdefault(key, {})[r["ItemOID"]] = (
            r["Value"] if r["Value"] not in (None, "") else None
        )


def age_years(birth, collected):
    b = date.fromisoformat(birth)
    c = date.fromisoformat(collected)
    years = c.year - b.year
    if (c.month, c.day) < (b.month, b.day):
        years -= 1
    return years


ranges = lbrange.to_dicts()

rows = []
for (subj, repkey), lab in labs.items():
    dm = subjects.get(subj, {})
    sex = dm.get("IT.DM.SEX")
    birth = dm.get("IT.DM.BRTHDTC")
    code = lab.get("IT.LB.LBTESTCD")
    lbnam = lab.get("IT.LB.LBNAM")
    lbdtc = lab.get("IT.LB.LBDTC")
    raw = lab.get("IT.LB.LBSTRESN")
    try:
        stresn = float(raw) if raw not in (None, "") else None
    except ValueError:
        stresn = None
    unit = lo = hi = None
    if birth and lbdtc and code and lbnam and sex:
        age = age_years(birth, lbdtc)
        best = None
        for rg in ranges:
            if rg["LBNAM"] != lbnam or rg["LBTESTCD"] != code or rg["SEX"] != sex:
                continue
            if not (rg["AGELO"] <= age <= rg["AGEHI"]):
                continue
            eff_start = rg["EFFSTDT"]
            eff_end = rg["EFFENDT"] if rg["EFFENDT"] not in (None, "") else None
            if lbdtc < eff_start:
                continue
            if eff_end is not None and lbdtc > eff_end:
                continue
            best = rg
            break
        if best is not None:
            unit, lo, hi = best["UNIT"], best["NRLO"], best["NRHI"]
    if stresn is None or lo is None or hi is None:
        nrind = None
    elif stresn < lo:
        nrind = "LOW"
    elif stresn > hi:
        nrind = "HIGH"
    else:
        nrind = "NORMAL"
    rows.append(
        {
            "DOMAIN": "LB",
            "STUDYID": study_of[subj],
            "USUBJID": subj,
            "LBSEQ": int(repkey),
            "LBTESTCD": code,
            "SEX": sex,
            "LBNAM": lbnam,
            "LBSTRESN": stresn,
            "LBORRESU": unit,
            "LBSTNRLO": lo,
            "LBSTNRHI": hi,
            "LBNRIND": nrind,
        }
    )

lb = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "LBSEQ": pl.Int64,
        "LBTESTCD": pl.String,
        "SEX": pl.String,
        "LBNAM": pl.String,
        "LBSTRESN": pl.Float64,
        "LBORRESU": pl.String,
        "LBSTNRLO": pl.Float64,
        "LBSTNRHI": pl.Float64,
        "LBNRIND": pl.String,
    },
).sort(["STUDYID", "USUBJID", "LBSEQ"])

Path("/app/output").mkdir(parents=True, exist_ok=True)
lb.write_csv("/app/output/lb.csv")
