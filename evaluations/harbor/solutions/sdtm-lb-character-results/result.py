# Reference solution for the yamaa benchmark sdtm-lb-character-results (Python track).
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
    key = (r["SubjectKey"], r["FormOID"], r["FormRepeatKey"])
    forms.setdefault(key, {})[r["ItemOID"]] = r["Value"]

form_info = {
    "FO.LB_PROT": ("PROT", "Protein", 0),
    "FO.LB_CK": ("CK", "Creatine Kinase", 1),
    "FO.LB_GLUC": ("GLUC", "Glucose", 2),
    "FO.LB_KETON": ("KETON", "Ketones", 3),
    "FO.LB_CREAT": ("CREAT", "Creatinine", 4),
}


def standardize(raw):
    if raw is None:
        return None
    t = raw.strip()
    low = t.lower().replace(" ", "")
    if low in ("negative", "neg"):
        return "NEGATIVE"
    if low in ("trace", "tr"):
        return "TRACE"
    if low in ("1+", "+1", "1plus"):
        return "1+"
    if low in ("2+", "+2", "2plus"):
        return "2+"
    return raw


def as_number(text):
    if text is None or text == "":
        return None
    try:
        return float(text)
    except ValueError:
        return None


rows = []
for (subj, formoid, formrep), items in forms.items():
    if formoid not in form_info:
        continue
    code, name, order = form_info[formoid]
    raw = items.get("IT.LB.RESULT")
    if raw is None or raw == "":
        continue
    lbdtc = items.get("IT.LB.LBDTC")
    unit = items.get("IT.LB.LBORRESU")
    unit = unit if unit not in (None, "") else None
    stresc = standardize(raw)
    # A true numeric result only: grades and censored values have no number.
    stresn = as_number(raw)
    if stresc in ("NEGATIVE", "TRACE", "1+", "2+"):
        stresn = None
    if raw.strip().startswith("<") or raw.strip().startswith(">"):
        stresn = None
    nrind = None
    if raw.strip().startswith("<"):
        nrind = "LOW"
    elif raw.strip().startswith(">"):
        nrind = "HIGH"
    rows.append(
        {
            "DOMAIN": "LB",
            "STUDYID": study_of[subj],
            "USUBJID": subj,
            "ORDER": order,
            "REP": int(formrep),
            "LBTESTCD": code,
            "LBTEST": name,
            "LBORRES": raw,
            "LBORRESU": unit,
            "LBSTRESC": stresc,
            "LBSTRESN": stresn,
            "LBSTRESU": unit,
            "LBNRIND": nrind,
            "LBDTC": lbdtc,
        }
    )

lb = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "ORDER": pl.Int64,
            "REP": pl.Int64,
            "LBTESTCD": pl.String,
            "LBTEST": pl.String,
            "LBORRES": pl.String,
            "LBORRESU": pl.String,
            "LBSTRESC": pl.String,
            "LBSTRESN": pl.Float64,
            "LBSTRESU": pl.String,
            "LBNRIND": pl.String,
            "LBDTC": pl.String,
        },
    )
    .sort(["STUDYID", "USUBJID", "ORDER", "REP"])
    .with_columns(
        pl.int_range(1, pl.len() + 1).over(["STUDYID", "USUBJID"]).alias("LBSEQ")
    )
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "LBTEST",
        "LBORRES",
        "LBORRESU",
        "LBSTRESC",
        "LBSTRESN",
        "LBSTRESU",
        "LBNRIND",
        "LBDTC",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
lb.write_csv("/app/output/lb.csv")
