# Reference solution for the yamaa benchmark adam-adtte-pfs (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def read(name: str) -> pl.DataFrame:
    return pl.read_csv(f"/app/input/{name}", infer_schema=False)


def first_per_subject(records: pl.DataFrame, by: list[str]) -> pl.DataFrame:
    return records.sort(by).unique("USUBJID", keep="first", maintain_order=True)


def last_per_subject(records: pl.DataFrame, by: list[str]) -> pl.DataFrame:
    return records.sort(by, descending=True).unique(
        "USUBJID", keep="first", maintain_order=True
    )


adsl = read("adsl.csv").with_columns(pl.col("RANDDT").str.to_date(strict=False))
rs = read("rs.csv").with_columns(
    pl.col("RSDTC").str.to_date(strict=False),
    pl.col("RSSEQ").cast(pl.Int64, strict=False),
)
ds = read("ds.csv").with_columns(
    pl.col("DSDTC").str.to_date(strict=False),
    pl.col("DSSEQ").cast(pl.Int64, strict=False),
)

# The first adequate progression, the first death, and the last adequate
# assessment. Records sharing a date are ordered by sequence number.
progression = first_per_subject(
    rs.filter(
        (pl.col("RSTESTCD") == "OVRLRESP")
        & (pl.col("RSSTRESC") == "PD")
        & (pl.col("ADEQFL") == "Y")
        & pl.col("RSDTC").is_not_null()
    ),
    ["RSDTC", "RSSEQ"],
).select("USUBJID", PDDT="RSDTC", PDSEQ="RSSEQ")

death = first_per_subject(
    ds.filter(
        (pl.col("DSDECOD") == "DEATH") & pl.col("DSDTC").is_not_null()
    ),
    ["DSDTC", "DSSEQ"],
).select("USUBJID", DTHDT="DSDTC", DTHSEQ="DSSEQ")

last_assessment = last_per_subject(
    rs.filter(
        (pl.col("RSTESTCD") == "OVRLRESP")
        & (pl.col("ADEQFL") == "Y")
        & pl.col("RSDTC").is_not_null()
    ),
    ["RSDTC", "RSSEQ"],
).select("USUBJID", CENSORDT="RSDTC", LASTSEQ="RSSEQ")

PDDT, DTHDT = pl.col("PDDT"), pl.col("DTHDT")

adtte = (
    adsl.join(progression, on="USUBJID", how="left")
    .join(death, on="USUBJID", how="left")
    .join(last_assessment, on="USUBJID", how="left")
    .with_columns(
        EVENTDT=pl.min_horizontal("PDDT", "DTHDT"),
        STARTDT=pl.col("RANDDT"),
        PARAMCD=pl.lit("PFS"),
        PARAM=pl.lit("Progression-Free Survival"),
    )
    # The event always wins, even after the last adequate assessment.
    .with_columns(
        CNSR=pl.when(pl.col("EVENTDT").is_not_null()).then(0).otherwise(1),
        ADT=pl.coalesce("EVENTDT", "CENSORDT"),
    )
    .with_columns(
        AVAL=(pl.col("ADT") - pl.col("STARTDT")).dt.total_days() + 1,
        # A progression and a death on the same day count as progression.
        EVNTDESC=pl.when(pl.col("EVENTDT").is_null())
        .then(pl.lit("CENSORED"))
        .when(PDDT.is_not_null() & (DTHDT.is_null() | (PDDT <= DTHDT)))
        .then(pl.lit("DISEASE PROGRESSION"))
        .otherwise(pl.lit("DEATH")),
    )
    .with_columns(
        SRCDOM=pl.when(pl.col("EVNTDESC") == "DEATH")
        .then(pl.lit("DS"))
        .otherwise(pl.lit("RS")),
        SRCVAR=pl.when(pl.col("EVNTDESC") == "DEATH")
        .then(pl.lit("DSDTC"))
        .otherwise(pl.lit("RSDTC")),
        SRCSEQ=pl.when(pl.col("EVNTDESC") == "DISEASE PROGRESSION")
        .then(pl.col("PDSEQ"))
        .when(pl.col("EVNTDESC") == "DEATH")
        .then(pl.col("DTHSEQ"))
        .otherwise(pl.col("LASTSEQ")),
    )
    # A record with no date has no trace; the fixture always has one.
    .with_columns(
        SRCDOM=pl.when(pl.col("ADT").is_null()).then(None).otherwise("SRCDOM"),
        SRCVAR=pl.when(pl.col("ADT").is_null()).then(None).otherwise("SRCVAR"),
        SRCSEQ=pl.when(pl.col("ADT").is_null()).then(None).otherwise("SRCSEQ"),
    )
    .sort("USUBJID")
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
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adtte.write_csv("/app/output/adtte.csv")
