# Reference solution for the yamaa benchmark adam-adae-protocol-review (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64), pl.col("SCORE").cast(pl.Float64)
)

adae = (
    ae.with_columns(
        ASTDT=pl.when(pl.col("AESTDTC").is_not_null() & (pl.col("AESTDTC") != ""))
        .then(pl.col("AESTDTC"))
        .otherwise(None),
        ASTDT2=pl.when(pl.col("AESTDTM").is_not_null() & (pl.col("AESTDTM") != ""))
        .then(pl.col("AESTDTM").str.extract(r"^(\d{4}-\d{2}-\d{2})", group_index=1))
        .otherwise(None),
    )
    .with_columns(
        REVIEWFL=pl.when(
            (
                pl.col("ASTDT").is_not_null()
                & pl.col("ASTDT").str.starts_with("2025-01")
                | (
                    pl.col("AESTDTM").is_not_null()
                    & (pl.col("AESTDTM") != "")
                    & (pl.col("AESTDTM") >= "2025-02-01T09:30")
                )
            )
            & pl.col("AETERM").is_not_null()
            & pl.col("AETERM").str.starts_with("INF_")
            & pl.col("SCORE").is_not_null()
            & (pl.col("SCORE") >= -1.5)
        )
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N"))
    )
    .select("STUDYID", "USUBJID", "AESEQ", "ASTDT", "ASTDT2", "REVIEWFL")
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
