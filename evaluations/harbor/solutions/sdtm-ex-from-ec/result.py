# Reference solution for the yamaa benchmark sdtm-ex-from-ec (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

odm = pl.read_csv("/app/input/odm.csv", infer_schema=False)
kit = pl.read_csv("/app/input/kit_list.csv", infer_schema=False)

kit_map = {r["KIT"]: (r["TREATMENT"], r["DOSE_MG"]) for r in kit.to_dicts()}

ec_forms = odm.filter(pl.col("FormOID") == "FO.EC")
vs_forms = odm.filter(pl.col("FormOID") == "FO.VS")

# Body weight per visit: SubjectKey + StudyEventOID + StudyEventRepeatKey.
vs_wide = (
    vs_forms.filter(pl.col("ItemOID") == "IT.VS.WEIGHT")
    .group_by("StudyOID", "SubjectKey", "StudyEventOID", "StudyEventRepeatKey")
    .agg(pl.col("Value").first().alias("WEIGHT"))
    .with_columns(pl.col("WEIGHT").cast(pl.Float64, strict=False))
)

ec_wide = (
    ec_forms.group_by(
        "StudyOID",
        "SubjectKey",
        "StudyEventOID",
        "StudyEventRepeatKey",
        "FormRepeatKey",
    )
    .agg(
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.ECOCCUR").first().alias("ECOCCUR"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.ECTRT").first().alias("ECTRT"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.FORM").first().alias("FORM"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.TABLETS").first().alias("TABLETS"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.STRENGTHMG").first().alias("STRENGTHMG"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.DOSEMKG").first().alias("DOSEMKG"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.KIT").first().alias("KIT"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.AUCTARGET").first().alias("AUCTARGET"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.ECSTDTC").first().alias("ECSTDTC"),
        pl.col("Value").filter(pl.col("ItemOID") == "IT.EC.ECENDTC").first().alias("ECENDTC"),
    )
    .join(
        vs_wide,
        on=["StudyOID", "SubjectKey", "StudyEventOID", "StudyEventRepeatKey"],
        how="left",
    )
    .filter(pl.col("ECOCCUR") == "Y")
)


def num(v):
    if v is None or v == "":
        return None
    try:
        return float(v)
    except ValueError:
        return None


records: list[dict] = []
for row in ec_wide.to_dicts():
    study = row["StudyOID"]
    subj = row["SubjectKey"]
    form = row["FORM"] or ""
    usubjid = subj
    if form == "KITDOSE":
        treatment, dose_mg = kit_map.get(row["KIT"] or "", (None, None))
        trt = treatment
        dose = num(dose_mg)
        dosu = "mg"
    elif form == "TABLET":
        trt = row["ECTRT"] or None
        tablets = num(row["TABLETS"])
        strength = num(row["STRENGTHMG"])
        dose = tablets * strength if tablets is not None and strength is not None else None
        dosu = "mg"
    elif form == "INFUSION":
        trt = row["ECTRT"] or None
        perkg = num(row["DOSEMKG"])
        weight = row["WEIGHT"]
        dose = perkg * weight if perkg is not None and weight is not None else None
        dosu = "mg"
    elif form == "AUCDOSE":
        trt = row["ECTRT"] or None
        dose = num(row["AUCTARGET"])
        dosu = "AUC"
    else:
        trt = row["ECTRT"] or None
        dose = None
        dosu = "mg"
    records.append(
        {
            "STUDYID": study,
            "USUBJID": usubjid,
            "EXTRT": trt,
            "EXDOSE": dose,
            "EXDOSU": dosu,
            "EXSTDTC": row["ECSTDTC"] or None,
            "EXENDTC": row["ECENDTC"] or None,
            "FORMREPEAT": int(row["FormRepeatKey"]) if row["FormRepeatKey"] else 0,
        }
    )

ex = (
    pl.DataFrame(
        records,
        schema={
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "EXTRT": pl.String,
            "EXDOSE": pl.Float64,
            "EXDOSU": pl.String,
            "EXSTDTC": pl.String,
            "EXENDTC": pl.String,
            "FORMREPEAT": pl.Int64,
        },
    )
    .with_columns(
        DOMAIN=pl.lit("EX"),
        EXSTDTC_DT=pl.col("EXSTDTC").str.to_date(strict=False),
    )
    .sort(["STUDYID", "USUBJID", "EXSTDTC_DT", "FORMREPEAT"])
    .with_columns(EXSEQ=pl.col("USUBJID").cum_count().over(["STUDYID", "USUBJID"]))
    .with_columns(EXSEQ=pl.col("EXSEQ").cast(pl.Int64))
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "EXSEQ",
        "EXTRT",
        "EXDOSE",
        "EXDOSU",
        "EXSTDTC",
        "EXENDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
ex.write_csv("/app/output/ex.csv")
