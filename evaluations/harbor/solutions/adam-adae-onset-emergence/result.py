# Reference solution for the yamaa benchmark adam-adae-onset-emergence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
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
adsl = (
    complete_seconds(adsl, "TRTSDTM", "TRTSDTM")
    .pipe(complete_seconds, "TEENDDTM", "TEENDDTM")
    .select("USUBJID", "TRTSDTM", "TEENDDTM")
)

# The window rule: onset inside [first exposure, window end] is
# treatment-emergent. A missing onset cannot be ruled pre-treatment,
# so a treated subject's event with a missing onset is flagged Y; an
# untreated subject (no first exposure) is never treatment-emergent.
adae = ae.join(adsl, on="USUBJID", how="left", maintain_order="left").with_columns(
    TRTEMFL=pl.when(pl.col("TRTSDTM").is_null())
    .then(None)
    .when(pl.col("ASTDTM").is_null())
    .then(pl.lit("Y"))
    .when(
        (pl.col("ASTDTM") >= pl.col("TRTSDTM"))
        & (pl.col("ASTDTM") <= pl.col("TEENDDTM"))
    )
    .then(pl.lit("Y"))
    .otherwise(None)
)

# The subject's earliest treatment-emergent event with a known onset,
# ordered by onset moment with the lower AESEQ settling ties at the
# same second. ISO datetimes sort as text once seconds are completed.
first = (
    adae.filter(
        (pl.col("TRTEMFL") == "Y") & pl.col("ASTDTM").is_not_null()
    )
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
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "ASTDTM",
        "TRTSDTM",
        "TEENDDTM",
        "TRTEMFL",
        "AOCCFL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
