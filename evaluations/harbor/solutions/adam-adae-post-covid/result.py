# Reference solution for the yamaa benchmark adam-adae-post-covid (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

# Each subject's first COVID-19 event: the earliest start date, with the
# higher sequence number ordered after on the same date. ISO dates sort
# as text.
first_covid = (
    ae.filter(
        (pl.col("AEDECOD") == "COVID-19") & pl.col("AESTDTC").is_not_null()
    )
    .sort(["USUBJID", "AESTDTC", "AESEQ"], nulls_last=True)
    .unique("USUBJID", keep="first", maintain_order=True)
    .select("USUBJID", FIRST_DT="AESTDTC", FIRST_SEQ="AESEQ")
)

adae = (
    ae.join(first_covid, on="USUBJID", how="left", maintain_order="left")
    .with_columns(ASTDT=pl.col("AESTDTC"))
    .with_columns(
        AFTCOVFL=pl.when(
            pl.col("ASTDT").is_not_null()
            & pl.col("FIRST_DT").is_not_null()
            & (
                (pl.col("ASTDT") > pl.col("FIRST_DT"))
                | (
                    (pl.col("ASTDT") == pl.col("FIRST_DT"))
                    & (pl.col("AESEQ") > pl.col("FIRST_SEQ"))
                )
            )
        ).then(pl.lit("Y"))
    )
    .select("STUDYID", "USUBJID", "AESEQ", "AEDECOD", "ASTDT", "AFTCOVFL")
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
