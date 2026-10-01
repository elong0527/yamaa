# Reference solution for the yamaa benchmark sdtm-ae-coding (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae_raw = pl.read_csv("/app/input/ae_raw.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
meddra = pl.read_csv("/app/input/meddra_26_1.csv", infer_schema=False)

# The preferred term and body system of the entry whose lowest-level term
# equals the reported term exactly, including letter case. A blank term or
# one with no exact match is not coded.
ae = (
    ae_raw.join(meddra, left_on="AETERM", right_on="LLTNAME", how="left")
    .with_columns(
        AEDECOD=pl.when(
            (pl.col("AETERM").fill_null("") == "") | pl.col("PTNAME").is_null()
        )
        .then(pl.lit("NOT CODED"))
        .otherwise(pl.col("PTNAME")),
        AEBODSYS=pl.when(
            (pl.col("AETERM").fill_null("") == "") | pl.col("SOCNAME").is_null()
        )
        .then(pl.lit("NOT CODED"))
        .otherwise(pl.col("SOCNAME")),
        DOMAIN=pl.lit("AE"),
    )
    .select("DOMAIN", "STUDYID", "USUBJID", "AESEQ", "AETERM", "AEDECOD", "AEBODSYS")
)

Path("/app/output").mkdir(exist_ok=True)
ae.write_csv("/app/output/ae.csv")
