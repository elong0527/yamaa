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


adsl = read("adsl.csv").with_columns(
    pl.col("RANDDT").str.to_date(strict=False),
    pl.col("NTXSTDT").str.to_date(strict=False),
)
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

# The last adequate assessment dated on or before new-therapy start, for
# subjects who began one.
pre_therapy = last_per_subject(
    rs.join(adsl.select("USUBJID", "NTXSTDT"), on="USUBJID", how="left").filter(
        (pl.col("RSTESTCD") == "OVRLRESP")
        & (pl.col("ADEQFL") == "Y")
        & pl.col("RSDTC").is_not_null()
        & pl.col("NTXSTDT").is_not_null()
        & (pl.col("RSDTC") <= pl.col("NTXSTDT"))
    ),
    ["RSDTC", "RSSEQ"],
).select("USUBJID", PRECENSORDT="RSDTC", PRESEQ="RSSEQ")

PDDT, DTHDT = pl.col("PDDT"), pl.col("DTHDT")

adtte = (
    adsl.join(progression, on="USUBJID", how="left")
    .join(death, on="USUBJID", how="left")
    .join(last_assessment, on="USUBJID", how="left")
    .join(pre_therapy, on="USUBJID", how="left")
    # A progression or death dated after new-therapy start is not an event.
    .with_columns(
        PDDT=pl.when(
            pl.col("NTXSTDT").is_not_null() & (pl.col("PDDT") > pl.col("NTXSTDT"))
        )
        .then(None)
        .otherwise("PDDT"),
        DTHDT=pl.when(
            pl.col("NTXSTDT").is_not_null() & (pl.col("DTHDT") > pl.col("NTXSTDT"))
        )
        .then(None)
        .otherwise("DTHDT"),
    )
    .with_columns(
        EVENTDT=pl.min_horizontal("PDDT", "DTHDT"),
        STARTDT=pl.col("RANDDT"),
        PARAMCD=pl.lit("PFS"),
        PARAM=pl.lit("Progression-Free Survival"),
        # The censoring assessment: the last adequate one overall, or the
        # last one dated on or before therapy start when therapy began.
        CENSORASSESSDT=pl.when(pl.col("NTXSTDT").is_null())
        .then(pl.col("CENSORDT"))
        .otherwise(pl.col("PRECENSORDT")),
        CENSORSEQ=pl.when(pl.col("NTXSTDT").is_null())
        .then(pl.col("LASTSEQ"))
        .otherwise(pl.col("PRESEQ")),
    )
    # With no usable adequate assessment the subject is censored at
    # randomization, for a 1-day PFS.
    .with_columns(
        CENSORDT=pl.coalesce("CENSORASSESSDT", "STARTDT"),
    )
    # The event always wins, even after the last adequate assessment,
    # unless new anti-cancer therapy started first.
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
        .when(pl.col("CENSORASSESSDT").is_null())
        .then(None)
        .otherwise(pl.lit("RS")),
        SRCVAR=pl.when(pl.col("EVNTDESC") == "DEATH")
        .then(pl.lit("DSDTC"))
        .when(pl.col("CENSORASSESSDT").is_null())
        .then(None)
        .otherwise(pl.lit("RSDTC")),
        SRCSEQ=pl.when(pl.col("EVNTDESC") == "DISEASE PROGRESSION")
        .then(pl.col("PDSEQ"))
        .when(pl.col("EVNTDESC") == "DEATH")
        .then(pl.col("DTHSEQ"))
        .when(pl.col("CENSORASSESSDT").is_null())
        .then(None)
        .otherwise(pl.col("CENSORSEQ")),
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
