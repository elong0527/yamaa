# Reference solution for the yamaa benchmark adam-adtte-pro-deterioration
# (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import calendar
from pathlib import Path

import polars as pl


def whole_months(d1, d2):
    """Whole calendar months from d1 to d2."""
    m = (d2.year - d1.year) * 12 + (d2.month - d1.month)
    last = calendar.monthrange(d2.year, d2.month)[1]
    if d2.day < d1.day and d2.day != last:
        m -= 1
    return m


adsl = pl.read_csv(
    "/app/input/adsl.csv", infer_schema=False, null_values=""
).with_columns(
    pl.col("RANDDT").str.to_date(strict=False),
    pl.col("DTHDT").str.to_date(strict=False),
)
qs = pl.read_csv("/app/input/qs.csv", infer_schema=False, null_values="").with_columns(
    pl.col("QSSEQ").cast(pl.Int64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
    pl.col("AVAL").cast(pl.Int64, strict=False),
)
ds = pl.read_csv("/app/input/ds.csv", infer_schema=False, null_values="").with_columns(
    pl.col("DSSEQ").cast(pl.Int64, strict=False),
    pl.col("DSDTC").str.to_date(strict=False),
)

# Flag deteriorations: the baseline is the earliest assessment, and an
# assessment deteriorates when it is dated after the baseline and its score
# is at least 10 points below the baseline score.
qs_flagged = (
    qs.sort(["STUDYID", "USUBJID", "ADT", "QSSEQ"], nulls_last=True)
    .with_columns(
        BASEDT=pl.col("ADT").first().over(["STUDYID", "USUBJID"]),
        BASEVAL=pl.col("AVAL").first().over(["STUDYID", "USUBJID"]),
    )
    .with_columns(
        DETERFL=pl.when(
            pl.col("ADT").is_not_null()
            & (pl.col("ADT") > pl.col("BASEDT"))
            & (pl.col("AVAL") <= pl.col("BASEVAL") - 10)
        )
        .then(pl.lit("Y"))
        .otherwise(None)
    )
    .select("STUDYID", "USUBJID", "QSSEQ", "AVISIT", "ADT", "AVAL", "DETERFL")
    .sort(["USUBJID", "QSSEQ"])
)

# The first deterioration per subject, earliest date then lowest sequence.
deter = (
    qs_flagged.filter(pl.col("DETERFL") == "Y")
    .sort(["STUDYID", "USUBJID", "ADT", "QSSEQ"])
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", DETERDT="ADT", DETERSEQ="QSSEQ")
)

# The first censoring reason per subject: progression, discontinuation, or
# withdrawal are censoring reasons, never events.
reason = (
    ds.sort(["STUDYID", "USUBJID", "DSDTC", "DSSEQ"])
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", CENSORRSNDT="DSDTC", CENSORRSN="DSDECOD")
)

# The latest assessment dated on or before the censoring reason, with the
# latest sequence breaking a tied date.
censor_qs = (
    qs_flagged.join(reason, on=["STUDYID", "USUBJID"], how="inner")
    .filter(pl.col("ADT").is_not_null() & (pl.col("ADT") <= pl.col("CENSORRSNDT")))
    .group_by(["STUDYID", "USUBJID"])
    .agg(
        LASTQSLE=pl.col("ADT").max(),
        LASTQSSEQ=pl.col("QSSEQ").filter(pl.col("ADT") == pl.col("ADT").max()).max(),
    )
)

adtte = (
    adsl.join(deter, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .join(reason, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .join(censor_qs, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(STARTDT=pl.col("RANDDT"))
    .with_columns(
        # Deterioration wins over death on the same date.
        EVENTDT=pl.when(
            pl.col("DETERDT").is_not_null()
            & (pl.col("DTHDT").is_null() | (pl.col("DETERDT") <= pl.col("DTHDT")))
        )
        .then(pl.col("DETERDT"))
        .when(pl.col("DTHDT").is_not_null())
        .then(pl.col("DTHDT"))
        .otherwise(None),
        CENSORDT=pl.col("LASTQSLE").fill_null(pl.col("STARTDT")),
    )
    .with_columns(ADT=pl.col("EVENTDT").fill_null(pl.col("CENSORDT")))
    .with_columns(
        AVAL=pl.struct(["STARTDT", "ADT"]).map_elements(
            lambda s: whole_months(s["STARTDT"], s["ADT"]), return_dtype=pl.Int64
        ),
        CNSR=pl.when(pl.col("EVENTDT").is_not_null()).then(0).otherwise(1),
        EVNTDESC=pl.when(
            pl.col("DETERDT").is_not_null()
            & (pl.col("DTHDT").is_null() | (pl.col("DETERDT") <= pl.col("DTHDT")))
        )
        .then(pl.lit("PRO DETERIORATION"))
        .when(pl.col("DTHDT").is_not_null())
        .then(pl.lit("DEATH"))
        .otherwise(pl.lit("CENSORED")),
        CNSDTDSC=pl.when(pl.col("EVENTDT").is_not_null())
        .then(None)
        .otherwise(pl.col("CENSORRSN").fill_null(pl.lit("STUDY COMPLETION"))),
        SRCDOM=pl.when(
            pl.col("DETERDT").is_not_null()
            & (pl.col("DTHDT").is_null() | (pl.col("DETERDT") <= pl.col("DTHDT")))
        )
        .then(pl.lit("QS"))
        .when(pl.col("DTHDT").is_not_null())
        .then(pl.lit("ADSL"))
        .when(pl.col("LASTQSLE").is_null())
        .then(pl.lit("ADSL"))
        .otherwise(pl.lit("QS")),
        SRCVAR=pl.when(
            pl.col("DETERDT").is_not_null()
            & (pl.col("DTHDT").is_null() | (pl.col("DETERDT") <= pl.col("DTHDT")))
        )
        .then(pl.lit("ADT"))
        .when(pl.col("DTHDT").is_not_null())
        .then(pl.lit("DTHDT"))
        .when(pl.col("LASTQSLE").is_null())
        .then(pl.lit("RANDDT"))
        .otherwise(pl.lit("ADT")),
        SRCSEQ=pl.when(
            pl.col("DETERDT").is_not_null()
            & (pl.col("DTHDT").is_null() | (pl.col("DETERDT") <= pl.col("DTHDT")))
        )
        .then(pl.col("DETERSEQ"))
        .when(pl.col("EVENTDT").is_null() & pl.col("LASTQSLE").is_not_null())
        .then(pl.col("LASTQSSEQ"))
        .otherwise(None),
        PARAMCD=pl.lit("TTDGHS"),
        PARAM=pl.lit("Time to Deterioration in Global Health Status"),
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
        "CNSDTDSC",
        "SRCDOM",
        "SRCVAR",
        "SRCSEQ",
    )
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
qs_flagged.write_csv("/app/output/qs_flagged.csv", null_value="")
adtte.write_csv("/app/output/adtte.csv", null_value="")
