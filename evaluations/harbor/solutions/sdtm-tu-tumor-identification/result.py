# Reference solution for the yamaa benchmark sdtm-tu-tumor-identification (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/tu_raw.csv", infer_schema=False).with_columns(
    pl.col("TUSEQ").cast(pl.Int64),
    pl.col("VISITNUM").cast(pl.Int64),
)

prefix = (
    pl.when(pl.col("LESION_CATEGORY") == "Target")
    .then(pl.lit("T"))
    .when(pl.col("LESION_CATEGORY") == "Non-target")
    .then(pl.lit("NT"))
    .otherwise(pl.lit("NEW"))
)
category = (
    pl.when(pl.col("LESION_CATEGORY") == "Target")
    .then(pl.lit("TARGET"))
    .when(pl.col("LESION_CATEGORY") == "Non-target")
    .then(pl.lit("NON-TARGET"))
    .otherwise(pl.lit("NEW"))
)

tu = (
    raw.with_columns(
        DOMAIN=pl.lit("TU"),
        TULNKID=prefix + pl.col("LESION_NUM"),
        TUTESTCD=pl.lit("TUMIDENT"),
        TUTEST=pl.lit("Tumor Identification"),
        TUORRES=category,
        TUSTRESC=category,
        TULOC=pl.col("LOCATION").str.to_uppercase(),
        TULAT=pl.col("LATERALITY").str.to_uppercase(),
        TUMETHOD=pl.col("METHOD"),
        TUEVAL=pl.col("EVALUATOR"),
    )
    .sort(["USUBJID", "TUSEQ"])
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "TUSEQ",
        "TULNKID",
        "TUTESTCD",
        "TUTEST",
        "TUORRES",
        "TUSTRESC",
        "TULOC",
        "TULAT",
        "TUMETHOD",
        "TUEVAL",
        "VISITNUM",
        "VISIT",
        "TUDTC",
    )
)

Path("/app/output").mkdir(exist_ok=True)
tu.write_csv("/app/output/tu.csv")
