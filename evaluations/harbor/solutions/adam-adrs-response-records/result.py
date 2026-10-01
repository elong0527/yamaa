# Reference solution for the yamaa benchmark adam-adrs-response-records (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).with_columns(
    pl.col("TRTSDT").str.to_date()
)
rs = pl.read_csv("/app/input/rs.csv", infer_schema=False).with_columns(
    pl.col("RSSEQ").cast(pl.Int64)
)

# Only investigator overall response assessments leave a record.
base = rs.filter(
    (pl.col("RSTESTCD") == "OVRLRESP") & (pl.col("RSEVAL") == "INVESTIGATOR")
).join(adsl.select("USUBJID", "TRTSDT"), on="USUBJID", how="left")

full = pl.col("RSDTC").str.contains(r"^\d{4}-\d{2}-\d{2}$")
year_month = pl.col("RSDTC").str.contains(r"^\d{4}-\d{2}$")
year_only = pl.col("RSDTC").str.contains(r"^\d{4}$")

# A completed date counts exactly as a collected one in every comparison.
adt = (
    pl.when(full)
    .then(pl.col("RSDTC").str.to_date(strict=False))
    .when(year_month)
    .then((pl.col("RSDTC") + "-01").str.to_date(strict=False))
    .when(year_only)
    .then((pl.col("RSDTC") + "-01-01").str.to_date(strict=False))
)

ady = (
    pl.when(pl.col("ADT").is_null() | pl.col("TRTSDT").is_null())
    .then(None)
    .when(pl.col("ADT") >= pl.col("TRTSDT"))
    .then((pl.col("ADT") - pl.col("TRTSDT")).dt.total_days() + 1)
    .otherwise((pl.col("ADT") - pl.col("TRTSDT")).dt.total_days())
)

derived = (
    base.with_columns(
        PARAMCD=pl.lit("OVR"),
        PARAM=pl.lit("Overall Response by Investigator"),
        ADT=adt,
    )
    .with_columns(
        ADY=ady.cast(pl.Int64),
        AVALC=pl.col("RSSTRESC"),
        AVAL=pl.col("RSSTRESC").replace_strict(
            {"CR": 1, "PR": 2, "SD": 3, "NON-CR/NON-PD": 4, "PD": 5, "NE": 6},
            default=None,
            return_dtype=pl.Int64,
        ),
    )
)

# One flagged record at each assessment date: the worst response that day,
# with the lowest sequence number breaking ties.
ranked = derived.sort(
    ["STUDYID", "USUBJID", "ADT", "AVAL", "RSSEQ"],
    descending=[False, False, False, True, False],
).with_columns(
    rank=pl.col("RSSEQ").cum_count().over(["STUDYID", "USUBJID", "ADT"])
)

adrs = (
    ranked.with_columns(
        ANL01FL=pl.when(pl.col("rank") == 1).then(pl.lit("Y")).otherwise(None)
    )
    .select(
        "STUDYID", "USUBJID", "RSSEQ", "PARAMCD", "PARAM", "RSDTC",
        "ADT", "ADY", "AVALC", "AVAL", "ANL01FL",
    )
    .sort(["USUBJID", "ADT", "RSSEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
adrs.write_csv("/app/output/adrs.csv")
