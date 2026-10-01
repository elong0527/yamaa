# Reference solution for the yamaa benchmark adam-adsl-new-therapy (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl_raw = pl.read_csv("/app/input/adsl.csv", infer_schema=False, null_values="").with_columns(
    TRTSDT=pl.col("TRTSDT").str.to_date(strict=False),
)
cm = pl.read_csv("/app/input/cm.csv", infer_schema=False, null_values="").with_columns(
    CMSTDTC_IMP=pl.col("CMSTDTC_IMP").str.to_date(strict=False),
)
pr = pl.read_csv("/app/input/pr.csv", infer_schema=False, null_values="").with_columns(
    PRSTDTC=pl.col("PRSTDTC").str.to_date(strict=False),
)

# A qualifying medication starts on or after treatment start; a record
# with no start date is skipped. The completed start fills a year-month
# to the 15th, so the IMP column is the comparison date.
cm_min = (
    cm.join(adsl_raw.select("STUDYID", "USUBJID", "TRTSDT"), on=["STUDYID", "USUBJID"], how="left")
    .filter(
        (pl.col("CMCAT") == "ON TREATMENT")
        & pl.col("CMSTDTC_IMP").is_not_null()
        & (pl.col("CMSTDTC_IMP") >= pl.col("TRTSDT"))
    )
    .group_by(["STUDYID", "USUBJID"])
    .agg(CMNACTDT=pl.col("CMSTDTC_IMP").min())
)

# A qualifying procedure is cancer related and on treatment, also on or
# after treatment start.
pr_min = (
    pr.join(adsl_raw.select("STUDYID", "USUBJID", "TRTSDT"), on=["STUDYID", "USUBJID"], how="left")
    .filter(
        (pl.col("PRCAT") == "CANCER RELATED")
        & (pl.col("PRSCAT") == "ON TREATMENT")
        & pl.col("PRSTDTC").is_not_null()
        & (pl.col("PRSTDTC") >= pl.col("TRTSDT"))
    )
    .group_by(["STUDYID", "USUBJID"])
    .agg(PRNACTDT=pl.col("PRSTDTC").min())
)

adsl = (
    adsl_raw.join(cm_min, on=["STUDYID", "USUBJID"], how="left")
    .join(pr_min, on=["STUDYID", "USUBJID"], how="left")
    .with_columns(
        NACTDT=pl.min_horizontal("CMNACTDT", "PRNACTDT"),
    )
    .with_columns(
        NACTDY=(pl.col("NACTDT") - pl.col("TRTSDT")).dt.total_days() + 1,
        NACTFL=pl.when(pl.col("NACTDT").is_not_null())
        .then(pl.lit("Y"))
        .otherwise(None),
    )
    .select("STUDYID", "USUBJID", "TRTSDT", "NACTDT", "NACTDY", "NACTFL")
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv", null_value="")
