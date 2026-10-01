# Reference solution for the yamaa benchmark adam-adlb-bds (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).with_columns(
    pl.col("TRTSDT").str.to_date(),
)
lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("LBSTRESN").cast(pl.Float64, strict=False).alias("RESULT"),
    pl.col("LBDTC").str.to_date().alias("ADT"),
)

# One record per collected ALT/AST result; a collected result with no
# numeric value produces no record.
collected = lb.filter(
    pl.col("LBTESTCD").is_in(["ALT", "AST"]) & pl.col("RESULT").is_not_null()
)

# ALT and AST hold the collected result and unit; ALTSI holds the ALT
# result times 0.0167 with unit ukat/L, one record for each ALT record.
alt = collected.filter(pl.col("LBTESTCD") == "ALT").with_columns(
    PARAMCD=pl.lit("ALT"),
    PARAM=pl.lit("Alanine Aminotransferase"),
    AVAL=pl.col("RESULT"),
    AVALU=pl.col("LBSTRESU"),
)
ast = collected.filter(pl.col("LBTESTCD") == "AST").with_columns(
    PARAMCD=pl.lit("AST"),
    PARAM=pl.lit("Aspartate Aminotransferase"),
    AVAL=pl.col("RESULT"),
    AVALU=pl.col("LBSTRESU"),
)
altsi = collected.filter(pl.col("LBTESTCD") == "ALT").with_columns(
    PARAMCD=pl.lit("ALTSI"),
    PARAM=pl.lit("Alanine Aminotransferase (SI)"),
    AVAL=pl.col("RESULT") * 0.0167,
    AVALU=pl.lit("ukat/L"),
)

base = (
    pl.concat([alt, ast, altsi], how="diagonal")
    .select(
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "ADT",
        "AVAL",
        "AVALU",
    )
    .join(
        adsl.select("STUDYID", "USUBJID", "TRTSDT", "TRT01A"),
        on=["STUDYID", "USUBJID"],
        how="left",
    )
)

# ABLFL is Y on the latest record on or before the treatment start date
# for each subject and parameter, including the start date itself.
baseline_dates = (
    base.filter(pl.col("ADT") <= pl.col("TRTSDT"))
    .group_by("STUDYID", "USUBJID", "PARAMCD")
    .agg(pl.col("ADT").max().alias("BASE_DT"))
)
with_flag = base.join(
    baseline_dates, on=["STUDYID", "USUBJID", "PARAMCD"], how="left"
).with_columns(
    ABLFL=pl.when(pl.col("ADT") == pl.col("BASE_DT"))
    .then(pl.lit("Y"))
    .otherwise(None)
)

# BASE repeats the flagged baseline value; CHG and PCHG follow, with PCHG
# empty when the baseline is zero. Values are not rounded.
baselines = (
    with_flag.filter(pl.col("ABLFL") == "Y")
    .select("STUDYID", "USUBJID", "PARAMCD", BASE="AVAL")
)
adlb_flagged = (
    with_flag.drop("BASE_DT")
    .join(baselines, on=["STUDYID", "USUBJID", "PARAMCD"], how="left")
    .with_columns(
        CHG=pl.col("AVAL") - pl.col("BASE"),
        PCHG=pl.when(pl.col("BASE").is_null() | (pl.col("BASE") == 0))
        .then(None)
        .otherwise(100 * (pl.col("AVAL") - pl.col("BASE")) / pl.col("BASE")),
    )
)

# ASEQ numbers each subject's records from 1, ordered by PARAMCD (ALT,
# then ALTSI, then AST) and then by ADT.
order = {"ALT": 0, "ALTSI": 1, "AST": 2}
adlb = (
    adlb_flagged.with_columns(
        PARAM_ORDER=pl.col("PARAMCD").replace_strict(order)
    )
    .sort(["STUDYID", "USUBJID", "PARAM_ORDER", "ADT"])
    .with_columns(
        ASEQ=pl.int_range(1, pl.len() + 1).over(["STUDYID", "USUBJID"]).cast(pl.Int64)
    )
    .sort(["PARAM_ORDER", "USUBJID", "ADT"])
    .select(
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "ADT",
        "TRTSDT",
        "TRT01A",
        "AVAL",
        "AVALU",
        "ABLFL",
        "BASE",
        "CHG",
        "PCHG",
        "ASEQ",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
