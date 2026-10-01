# Reference solution for the yamaa benchmark adam-adae-occurrence-flags (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/adae_raw.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

# The first treatment-emergent event at each level: the earliest start
# date, with the lower AESEQ breaking ties on the same day. ISO dates
# sort as text.
first_overall = (
    ae.filter(pl.col("TRTEMFL") == "Y")
    .sort(["USUBJID", "ASTDT", "AESEQ"], nulls_last=True)
    .unique("USUBJID", keep="first", maintain_order=True)
    .select("USUBJID", FIRST_SEQ="AESEQ")
)

first_soc = (
    ae.filter(pl.col("TRTEMFL") == "Y")
    .sort(["USUBJID", "AEBODSYS", "ASTDT", "AESEQ"], nulls_last=True)
    .unique(["USUBJID", "AEBODSYS"], keep="first", maintain_order=True)
    .select("USUBJID", "AEBODSYS", SOC_SEQ="AESEQ")
)

first_pt = (
    ae.filter(pl.col("TRTEMFL") == "Y")
    .sort(["USUBJID", "AEBODSYS", "AEDECOD", "ASTDT", "AESEQ"], nulls_last=True)
    .unique(["USUBJID", "AEBODSYS", "AEDECOD"], keep="first", maintain_order=True)
    .select("USUBJID", "AEBODSYS", "AEDECOD", PT_SEQ="AESEQ")
)

adae = (
    ae.join(first_overall, on="USUBJID", how="left")
    .join(first_soc, on=["USUBJID", "AEBODSYS"], how="left")
    .join(first_pt, on=["USUBJID", "AEBODSYS", "AEDECOD"], how="left")
    .with_columns(
        AOCCFL=pl.when(pl.col("FIRST_SEQ").is_not_null() & (pl.col("AESEQ") == pl.col("FIRST_SEQ")))
        .then(pl.lit("Y")),
        AOCCSFL=pl.when(pl.col("SOC_SEQ").is_not_null() & (pl.col("AESEQ") == pl.col("SOC_SEQ")))
        .then(pl.lit("Y")),
        AOCCPFL=pl.when(pl.col("PT_SEQ").is_not_null() & (pl.col("AESEQ") == pl.col("PT_SEQ")))
        .then(pl.lit("Y")),
    )
    .select(
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AEBODSYS",
        "AEDECOD",
        "ASTDT",
        "TRTEMFL",
        "AOCCFL",
        "AOCCSFL",
        "AOCCPFL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
