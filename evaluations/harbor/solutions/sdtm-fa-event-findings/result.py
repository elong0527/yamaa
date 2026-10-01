# Reference solution for the yamaa benchmark sdtm-fa-event-findings (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)

ae_by_occurrence = {}
fa_forms = {}
study_of = {}
for r in odm.to_dicts():
    study_of[r["SubjectKey"]] = r["StudyOID"]
    if r["ItemGroupOID"] == "IG.AE":
        key = (r["SubjectKey"], r["StudyEventOID"], r["StudyEventRepeatKey"])
        ae_by_occurrence.setdefault(key, {})[r["ItemOID"]] = (
            r["Value"] if r["Value"] not in (None, "") else None
        )
    elif r["ItemGroupOID"] == "IG.FA":
        key = (
            r["SubjectKey"],
            r["StudyEventOID"],
            r["StudyEventRepeatKey"],
            r["ItemGroupRepeatKey"],
        )
        fa_forms.setdefault(key, {})[r["ItemOID"]] = (
            r["Value"] if r["Value"] not in (None, "") else None
        )

test_order = {"IT.FA.LOCATION": 0, "IT.FA.SIZE": 1, "IT.FA.BIOPSY": 2}
test_info = {
    "IT.FA.LOCATION": ("LOC", "Location"),
    "IT.FA.SIZE": ("SIZE", "Size"),
    "IT.FA.BIOPSY": ("BIOPSY", "Biopsied"),
}

rows = []
for (subj, event, rep, _), form in fa_forms.items():
    ae = ae_by_occurrence.get((subj, event, rep), {})
    aeterm = ae.get("IT.AE.AETERM")
    if not aeterm:
        continue
    link = form.get("IT.FA.AELNKID") or ae.get("IT.AE.AELNKID")
    fadtc = form.get("IT.FA.FADTC")
    sizeu = form.get("IT.FA.SIZEU")
    for oid in ("IT.FA.LOCATION", "IT.FA.SIZE", "IT.FA.BIOPSY"):
        val = form.get(oid)
        if val is None:
            continue
        code, name = test_info[oid]
        is_size = oid == "IT.FA.SIZE"
        try:
            num = float(val) if is_size else None
        except ValueError:
            num = None
        rows.append(
            {
                "DOMAIN": "FA",
                "STUDYID": study_of[subj],
                "USUBJID": subj,
                "LINK": link,
                "ORDER": test_order[oid],
                "TIE": val,
                "FATESTCD": code,
                "FATEST": name,
                "FAOBJ": aeterm,
                "FACAT": "AE",
                "FAORRES": val,
                "FAORRESU": sizeu if is_size else None,
                "FASTRESC": val,
                "FASTRESN": num,
                "FASTRESU": sizeu if is_size else None,
                "FALNKID": link,
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
            "LINK": pl.String,
            "ORDER": pl.Int64,
            "TIE": pl.String,
            "FATESTCD": pl.String,
            "FATEST": pl.String,
            "FAOBJ": pl.String,
            "FACAT": pl.String,
            "FAORRES": pl.String,
            "FAORRESU": pl.String,
            "FASTRESC": pl.String,
            "FASTRESN": pl.Float64,
            "FASTRESU": pl.String,
            "FALNKID": pl.String,
            "FADTC": pl.String,
        },
    )
    .sort(["STUDYID", "USUBJID", "LINK", "ORDER", "TIE"])
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
        "FASTRESN",
        "FASTRESU",
        "FALNKID",
        "FADTC",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
fa.write_csv("/app/output/fa.csv")
