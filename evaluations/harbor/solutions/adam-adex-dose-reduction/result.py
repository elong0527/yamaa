# Reference solution for the yamaa benchmark adam-adex-dose-reduction (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ex = pl.read_csv("/app/input/ex.csv", infer_schema=False).with_columns(
    pl.col("EXSEQ").cast(pl.Int64)
)

adex = (
    ex.with_columns(
        EXSTDTM=pl.when(pl.col("EXSTDTM").str.len_chars() == 16)
        .then(pl.col("EXSTDTM") + ":00")
        .otherwise(pl.col("EXSTDTM")),
        EXDOSE=pl.col("EXDOSE").cast(pl.Float64, strict=False),
    )
    # Chronological treatment-start order within each subject, breaking
    # timestamp ties by sequence number. ISO datetimes sort as text.
    .sort(["USUBJID", "EXSTDTM", "EXSEQ"])
    .with_columns(PREVDOSE=pl.col("EXDOSE").shift(1).over("USUBJID"))
    # Y when the current dose is lower than the previous one; no value for
    # the first record and whenever either dose is zero or missing. The
    # previous dose is always the administration just before.
    .with_columns(
        DOSREDFL=pl.when(
            pl.col("EXDOSE").is_not_null()
            & (pl.col("EXDOSE") != 0)
            & pl.col("PREVDOSE").is_not_null()
            & (pl.col("PREVDOSE") != 0)
            & (pl.col("EXDOSE") < pl.col("PREVDOSE"))
        )
        .then(pl.lit("Y"))
        .otherwise(None)
    )
    .select("STUDYID", "USUBJID", "EXSEQ", "EXSTDTM", "EXDOSE", "DOSREDFL")
)

Path("/app/output").mkdir(exist_ok=True)
adex.write_csv("/app/output/adex.csv")
