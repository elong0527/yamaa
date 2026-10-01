# Reference solution for the yamaa benchmark adam-adcm-on-treatment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

cm = pl.read_csv("/app/input/cm.csv", infer_schema=False).with_columns(
    pl.col("CMSEQ").cast(pl.Int64)
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).select(
    "USUBJID", "TRTSDT", "TRTEDT"
)

adcm = cm.join(adsl, on="USUBJID", how="left").with_columns(
    ASTDT=pl.col("CMSTDTC"), AENDT=pl.col("CMENDTC")
)

# The medication overlaps the treatment period. A missing medication date is
# assumed to overlap unless the known dates rule it out; a missing treatment
# end leaves the period open, while no treatment start is never flagged.
# ISO dates sort as text.
adcm = (
    adcm.with_columns(
        ONTRTFL=pl.when(pl.col("TRTSDT").is_null() | (pl.col("TRTSDT") == ""))
        .then(None)
        .when(
            pl.col("AENDT").is_not_null()
            & (pl.col("AENDT") != "")
            & (pl.col("AENDT") < pl.col("TRTSDT"))
        )
        .then(None)
        .when(
            pl.col("ASTDT").is_not_null()
            & (pl.col("ASTDT") != "")
            & pl.col("TRTEDT").is_not_null()
            & (pl.col("TRTEDT") != "")
            & (pl.col("ASTDT") > pl.col("TRTEDT"))
        )
        .then(None)
        .otherwise(pl.lit("Y"))
    ).select(
        "STUDYID",
        "USUBJID",
        "CMSEQ",
        "CMTRT",
        "ASTDT",
        "AENDT",
        "TRTSDT",
        "TRTEDT",
        "ONTRTFL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adcm.write_csv("/app/output/adcm.csv")
