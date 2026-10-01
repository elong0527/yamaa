# Reference solution for the yamaa benchmark adam-adae-onset-emergence (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False)


def complete_seconds(frame: pl.DataFrame, name: str, out: str) -> pl.DataFrame:
    # Datetimes collected without seconds are completed to the second so
    # the onset and the first exposure compare at the moment, not the day.
    return frame.with_columns(
        pl.when(pl.col(name).str.contains(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$"))
        .then(pl.col(name) + ":00")
        .otherwise(pl.col(name))
        .alias(out)
    )


ae = complete_seconds(ae, "AESTDTC", "ASTDTM").select(
    "STUDYID", "USUBJID", "AESEQ", "AETERM", "ASTDTM"
)
adsl = complete_seconds(adsl, "TRTSDTM", "TRTSDTM").select("USUBJID", "TRTSDTM")

adae = ae.join(adsl, on="USUBJID", how="left", maintain_order="left").with_columns(
    TRTEMFL=pl.when(
        pl.col("ASTDTM").is_not_null()
        & pl.col("TRTSDTM").is_not_null()
        & (pl.col("ASTDTM") >= pl.col("TRTSDTM"))
    ).then(pl.lit("Y"))
)

# The subject's earliest treatment-emergent event, ordered by onset
# moment with the lower AESEQ settling ties at the same second. ISO
# datetimes sort as text once seconds are completed.
first = (
    adae.filter(pl.col("TRTEMFL") == "Y")
    .sort(["USUBJID", "ASTDTM", "AESEQ"], nulls_last=True)
    .unique("USUBJID", keep="first", maintain_order=True)
    .select("USUBJID", FIRST_SEQ="AESEQ")
)

adae = (
    adae.join(first, on="USUBJID", how="left")
    .with_columns(
        AOCCFL=pl.when(
            pl.col("FIRST_SEQ").is_not_null() & (pl.col("AESEQ") == pl.col("FIRST_SEQ"))
        ).then(pl.lit("Y"))
    )
    .select(
        "STUDYID", "USUBJID", "AESEQ", "AETERM", "ASTDTM", "TRTSDTM", "TRTEMFL", "AOCCFL"
    )
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
