# Reference solution for the yamaa benchmark adam-adcm-atc-classes (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

cm = pl.read_csv("/app/input/cm.csv", infer_schema=False).with_columns(
    pl.col("CMSEQ").cast(pl.Int64)
)
facm = pl.read_csv("/app/input/facm.csv", infer_schema=False).with_columns(
    pl.col("CMSEQ").cast(pl.Int64, strict=False)
)
atcdict = pl.read_csv("/app/input/atc_dict.csv", infer_schema=False)

levels = ["ATC1", "ATC2", "ATC3", "ATC4"]

# One ATC level per finding row; rows naming no level from ATC1 to ATC4, or
# pointing at no medication record, change nothing.
codes = (
    facm.filter(pl.col("FATESTCD").is_in(levels))
    .pivot(
        index=["USUBJID", "CMSEQ"],
        on="FATESTCD",
        values="FAORRES",
        aggregate_function="first",
    )
    .rename({level: f"{level}CD" for level in levels})
)

lookup = atcdict.select("ATCCODE", "ATCNAME")

adcm = cm.join(codes, on=["USUBJID", "CMSEQ"], how="left")

# A medication with no coded name keeps every ATC column empty.
uncoded = pl.col("CMDECOD").is_null() | (pl.col("CMDECOD") == "")
for level in levels:
    code = f"{level}CD"
    if code not in adcm.columns:
        adcm = adcm.with_columns(pl.lit(None).cast(pl.String).alias(code))
    adcm = adcm.with_columns(
        pl.when(uncoded).then(None).otherwise(pl.col(code)).alias(code)
    )

for level in levels:
    code, name = f"{level}CD", level
    adcm = (
        adcm.join(
            lookup.rename({"ATCCODE": code, "ATCNAME": name}),
            on=code,
            how="left",
        )
    )

adcm = adcm.select(
    "STUDYID",
    "USUBJID",
    "CMSEQ",
    "CMTRT",
    "CMDECOD",
    "ATC1",
    "ATC2",
    "ATC3",
    "ATC4",
    "ATC1CD",
    "ATC2CD",
    "ATC3CD",
    "ATC4CD",
)

Path("/app/output").mkdir(exist_ok=True)
adcm.write_csv("/app/output/adcm.csv")
