# Reference solution for the yamaa benchmark adam-adfa-fever (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64),
    pl.col("VSSTRESN").cast(pl.Float64, strict=False),
)

# Only temperature records in the reactogenicity category qualify.
temp = vs.filter(
    (pl.col("VSTESTCD") == "TEMP") & (pl.col("VSCAT") == "REACTOGENICITY")
).with_columns(
    pl.when((pl.col("VSSTRESU") == "C") & (pl.col("VSSTRESN") >= 38))
    .then(pl.lit("Y"))
    .when((pl.col("VSSTRESU") == "C") & (pl.col("VSSTRESN") < 38))
    .then(pl.lit("N"))
    .alias("AVALC"),
)

adfa = (
    temp.with_columns(
        pl.lit("ADFA").alias("DOMAIN"),
        pl.lit("FEVER").alias("PARAMCD"),
        pl.lit("Fever Occurrence").alias("PARAM"),
        pl.when(pl.col("AVALC") == "Y")
        .then(1.0)
        .when(pl.col("AVALC") == "N")
        .then(0.0)
        .alias("AVAL"),
        pl.col("VSDTC").str.to_date("%Y-%m-%dT%H:%M").alias("ADT"),
        pl.lit("VS").alias("SRCDOM"),
        pl.lit("VSSTRESN").alias("SRCVAR"),
        pl.col("VSSEQ").alias("SRCSEQ"),
    )
    .sort(["USUBJID", "VSDTC", "VSSEQ"])
    # Number each subject's records 1, 2, 3 ... in assessment-date order.
    .with_columns(pl.col("USUBJID").cum_count().over("USUBJID").alias("ASEQ"))
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "ASEQ",
        "PARAMCD",
        "PARAM",
        "AVAL",
        "AVALC",
        "ADT",
        "SRCDOM",
        "SRCVAR",
        "SRCSEQ",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adfa.write_csv("/app/output/adfa.csv")
