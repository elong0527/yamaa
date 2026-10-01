# Reference solution for the yamaa benchmark adam-adtte-first-ae (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False, null_values="").with_columns(
    pl.col("TRTSDT").str.to_date(strict=False),
    pl.col("EOSDT").str.to_date(strict=False),
)
adae = pl.read_csv("/app/input/adae.csv", infer_schema=False, null_values="").with_columns(
    pl.col("AESEQ").cast(pl.Int64, strict=False),
    pl.col("ASTDT").str.to_date(strict=False),
)

# The subject's first adverse event by onset date, ties to the lower
# sequence; an event with no onset sorts last, so it is passed over when
# a dated event exists.
first_ae = (
    adae.filter(pl.col("ASTDT").is_not_null())
    .sort(["STUDYID", "USUBJID", "ASTDT", "AESEQ"])
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", AEDT="ASTDT", AESEQ="AESEQ")
)

adtte = (
    adsl.join(first_ae, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(
        STARTDT=pl.col("TRTSDT"),
        # The earliest onset, or the end of study when no event carries
        # a usable onset date.
        RAWDT=pl.coalesce("AEDT", "EOSDT"),
    )
    .with_columns(
        # A date before treatment start is moved up to it, keeping its
        # event-or-censoring status and source; with no treatment start
        # the date is kept as it is.
        ADT=pl.when(pl.col("STARTDT") > pl.col("RAWDT"))
        .then(pl.col("STARTDT"))
        .otherwise(pl.col("RAWDT")),
    )
    .with_columns(
        AVAL=(pl.col("ADT") - pl.col("STARTDT")).dt.total_days() + 1,
        CNSR=pl.when(pl.col("AEDT").is_not_null()).then(0).otherwise(1),
        EVNTDESC=pl.when(pl.col("AEDT").is_not_null())
        .then(pl.lit("AE"))
        .otherwise(pl.lit("END OF STUDY")),
        SRCDOM=pl.when(pl.col("AEDT").is_not_null())
        .then(pl.lit("ADAE"))
        .otherwise(pl.lit("ADSL")),
        SRCVAR=pl.when(pl.col("AEDT").is_not_null())
        .then(pl.lit("ASTDT"))
        .otherwise(pl.lit("EOSDT")),
        SRCSEQ=pl.when(pl.col("AEDT").is_not_null())
        .then(pl.col("AESEQ"))
        .otherwise(None),
        PARAMCD=pl.lit("TTAE"),
        PARAM=pl.lit("Time to First Adverse Event"),
    )
    .select(
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "STARTDT",
        "ADT",
        "AVAL",
        "CNSR",
        "EVNTDESC",
        "SRCDOM",
        "SRCVAR",
        "SRCSEQ",
    )
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adtte.write_csv("/app/output/adtte.csv", null_value="")
