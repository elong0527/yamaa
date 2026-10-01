# Reference solution for the yamaa benchmark sdtm-ae-meddra-hierarchy (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae_raw = pl.read_csv("/app/input/ae_raw.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
meddra = pl.read_csv("/app/input/meddra_synthetic.csv", infer_schema=False)

# Every hierarchy field comes from the primary path for the coder-assigned
# code; the extract marks it with PRIMARY_SOC Y. An unknown or absent code
# leaves the hierarchy empty, but the assigned code stays in AELLTCD.
primary = meddra.filter(pl.col("PRIMARY_SOC") == "Y")

ae = (
    ae_raw.join(primary, left_on="AELLTCD", right_on="LLTCD", how="left")
    .with_columns(
        DOMAIN=pl.lit("AE"),
        AELLT=pl.col("LLTNAME"),
        AEDECOD=pl.col("PTNAME"),
        AEPTCD=pl.col("PTCD"),
        AEHLT=pl.col("HLTNAME"),
        AEHLTCD=pl.col("HLTCD"),
        AEHLGT=pl.col("HLGTNAME"),
        AEHLGTCD=pl.col("HLGTCD"),
        AEBODSYS=pl.col("SOCNAME"),
        AEBODSCD=pl.col("SOCCD"),
        AESOC=pl.col("SOCNAME"),
        AESOCCD=pl.col("SOCCD"),
    )
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "AELLT",
        "AELLTCD",
        "AEDECOD",
        "AEPTCD",
        "AEHLT",
        "AEHLTCD",
        "AEHLGT",
        "AEHLGTCD",
        "AEBODSYS",
        "AEBODSCD",
        "AESOC",
        "AESOCCD",
    )
)

Path("/app/output").mkdir(exist_ok=True)
ae.write_csv("/app/output/ae.csv")
