# Reference solution for the yamaa benchmark adam-adae-worst-severity (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adae_raw = (
    pl.read_csv("/app/input/adae_raw.csv", infer_schema=False)
    .with_columns(
        pl.col("AESEQ").cast(pl.Int64),
        pl.col("ASTDT").str.to_date(strict=False),
    )
)

adae = adae_raw.with_columns(
    AESEVN=pl.when(pl.col("AESEV") == "MILD")
    .then(1)
    .when(pl.col("AESEV") == "MODERATE")
    .then(2)
    .when(pl.col("AESEV") == "SEVERE")
    .then(3)
    .otherwise(None)
    .cast(pl.Int64)
)

# The eligible event with the greatest rank per subject and term; ties break
# by earliest date, then lowest sequence. Only a treatment-emergent event
# with a graded severity is eligible. ISO dates sort as text.
best = (
    adae.filter(pl.col("TRTEMFL") == "Y", pl.col("AESEVN").is_not_null())
    .sort(["AESEVN", "ASTDT", "AESEQ"], descending=[True, False, False])
    .unique(["USUBJID", "AEDECOD"], keep="first", maintain_order=True)
    .select("USUBJID", "AEDECOD", "AESEQ")
    .with_columns(BEST=pl.lit("Y"))
)

adae = (
    adae.join(best, on=["USUBJID", "AEDECOD", "AESEQ"], how="left")
    .with_columns(AWSEVFL=pl.col("BEST"))
    .select(
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AEBODSYS",
        "AEDECOD",
        "ASTDT",
        "AESEV",
        "TRTEMFL",
        "AESEVN",
        "AWSEVFL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
