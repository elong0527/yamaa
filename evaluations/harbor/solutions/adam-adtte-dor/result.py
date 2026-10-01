# Reference solution for the yamaa benchmark adam-adtte-dor (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def read(name: str) -> pl.DataFrame:
    return pl.read_csv(f"/app/input/{name}", infer_schema=False)


def first_per_subject(
    records: pl.DataFrame, by: list[str], descending: bool = False
) -> pl.DataFrame:
    return records.sort(by, descending=descending).unique(
        "USUBJID", keep="first", maintain_order=True
    )


adsl = read("adsl.csv").with_columns(pl.col("RESPDT", "NACTDT").str.to_date())
adrs = read("adrs_raw.csv").with_columns(
    pl.col("ADT").str.to_date(), pl.col("ASEQ").cast(pl.Int64)
)
ds = read("ds.csv").with_columns(
    pl.col("DSSTDTC").str.to_date(), pl.col("DSSEQ").cast(pl.Int64)
)

# Earliest progression and death, lowest sequence number on a tie.
progression = first_per_subject(
    adrs.filter(pl.col("AVALC") == "PD", pl.col("ADT").is_not_null()),
    ["ADT", "ASEQ"],
).select("USUBJID", PDDT="ADT", PDSEQ="ASEQ")

death = first_per_subject(
    ds.filter(pl.col("DSDECOD") == "DEATH", pl.col("DSSTDTC").is_not_null()),
    ["DSSTDTC", "DSSEQ"],
).select("USUBJID", DTHDT="DSSTDTC", DTHSEQ="DSSEQ")

# Last evaluable assessment (a date and a response other than NE), highest
# sequence number on a tie.
last_assessment = first_per_subject(
    adrs.filter(pl.col("ADT").is_not_null(), pl.col("AVALC") != "NE"),
    ["ADT", "ASEQ"],
    descending=True,
).select("USUBJID", LASTDT="ADT", LASTSEQ="ASEQ")

# Where ADT comes from, for each outcome.
sources = pl.DataFrame(
    {
        "OUTCOME": [
            "DISEASE PROGRESSION",
            "DEATH",
            "LAST TUMOUR ASSESSMENT",
            "START OF NEW ANTI-CANCER THERAPY",
        ],
        "SRCDOM": ["ADRS", "DS", "ADRS", "ADSL"],
        "SRCVAR": ["ADT", "DSSTDTC", "ADT", "NACTDT"],
    }
)

PDDT, DTHDT, LASTDT, NACTDT = (
    pl.col(name) for name in ("PDDT", "DTHDT", "LASTDT", "NACTDT")
)
EVENT, OUTCOME = pl.col("EVENT"), pl.col("OUTCOME")

adtte = (
    adsl.filter(pl.col("RESPDT").is_not_null())
    .join(progression, on="USUBJID", how="left")
    .join(death, on="USUBJID", how="left")
    .join(last_assessment, on="USUBJID", how="left")
    # The first event; progression wins a same-day tie with death.
    .with_columns(
        EVENT=pl.when(PDDT.is_not_null() & (DTHDT.is_null() | (PDDT <= DTHDT)))
        .then(pl.lit("DISEASE PROGRESSION"))
        .when(DTHDT.is_not_null())
        .then(pl.lit("DEATH"))
    )
    .with_columns(EVENTDT=pl.when(EVENT == "DEATH").then(DTHDT).otherwise(PDDT))
    # An event after the start of new therapy does not count.
    .with_columns(
        EVENT=pl.when(NACTDT.is_not_null() & (pl.col("EVENTDT") > NACTDT))
        .then(None)
        .otherwise(EVENT)
    )
    # Without a counted event, the record is censored at the earlier of the
    # last evaluable assessment and the start of new therapy.
    .with_columns(
        OUTCOME=pl.when(EVENT.is_not_null())
        .then(EVENT)
        .when(LASTDT.is_not_null() & (NACTDT.is_null() | (LASTDT <= NACTDT)))
        .then(pl.lit("LAST TUMOUR ASSESSMENT"))
        .when(NACTDT.is_not_null())
        .then(pl.lit("START OF NEW ANTI-CANCER THERAPY"))
    )
    .with_columns(
        ADT=pl.when(OUTCOME == "DISEASE PROGRESSION")
        .then(PDDT)
        .when(OUTCOME == "DEATH")
        .then(DTHDT)
        .when(OUTCOME == "LAST TUMOUR ASSESSMENT")
        .then(LASTDT)
        .when(OUTCOME == "START OF NEW ANTI-CANCER THERAPY")
        .then(NACTDT),
        SRCSEQ=pl.when(OUTCOME == "DISEASE PROGRESSION")
        .then(pl.col("PDSEQ"))
        .when(OUTCOME == "DEATH")
        .then(pl.col("DTHSEQ"))
        .when(OUTCOME == "LAST TUMOUR ASSESSMENT")
        .then(pl.col("LASTSEQ")),
        PARAMCD=pl.lit("DOR"),
        PARAM=pl.lit("Duration of Response"),
        STARTDT=pl.col("RESPDT"),
        CNSR=pl.when(EVENT.is_null()).then(1).otherwise(0),
        EVNTDESC=pl.coalesce(EVENT, pl.lit("CENSORED")),
        CNSDTDSC=pl.when(EVENT.is_null()).then(OUTCOME),
    )
    .with_columns(AVAL=(pl.col("ADT") - pl.col("STARTDT")).dt.total_days() + 1)
    .join(sources, on="OUTCOME", how="left")
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
)

Path("/app/output").mkdir(exist_ok=True)
adtte.write_csv("/app/output/adtte.csv")
