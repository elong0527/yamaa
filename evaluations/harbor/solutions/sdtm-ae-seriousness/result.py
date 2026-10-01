# Reference solution for the yamaa benchmark sdtm-ae-seriousness (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae_raw = pl.read_csv("/app/input/ae_raw.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

criteria = ["AESDTH", "AESLIFE", "AESHOSP", "AESDISAB", "AESCONG", "AESMIE"]

# Serious when any criterion is Y; otherwise not serious, even when every
# criterion is empty. The criterion flags stay as collected.
ae = (
    ae_raw.with_columns(
        AESER=pl.when(
            pl.any_horizontal(pl.col(criteria) == "Y")
        )
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N")),
        DOMAIN=pl.lit("AE"),
    ).select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "AESER",
        *criteria,
    )
)

Path("/app/output").mkdir(exist_ok=True)
ae.write_csv("/app/output/ae.csv")
