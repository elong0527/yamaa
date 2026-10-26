# Reference solution for the yamaa benchmark adam-adqs-pro-phantom-records (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

COLUMNS = [
    "STUDYID", "USUBJID", "PARCAT2", "PARAMCD", "PARAM", "AVISIT", "AVISITN",
    "AVAL", "DTYPE", "QSSTAT", "QSREASND", "AREASND", "PROEX1FL", "PROEX2FL",
    "PROSCMFL",
]
ITEMS = ["FAT01", "FAT02"]
TOTAL = "FATTOT"

STR = pl.Utf8
adsl = pl.read_csv(
    INPUT / "adsl.csv",
    schema_overrides={c: STR for c in
                      ["STUDYID", "USUBJID", "RANDDT", "SAFFL", "EOTDT", "DCTREAS", "DTHDT"]},
).with_columns(
    [pl.col(c).str.strip_chars().replace("", None).str.to_date("%Y-%m-%d", strict=False)
     for c in ["RANDDT", "EOTDT", "DTHDT"]]
)
qs = pl.read_csv(
    INPUT / "qs.csv",
    schema_overrides={
        "STUDYID": STR, "USUBJID": STR, "QSTESTCD": STR, "QSSTAT": STR,
        "QSREASND": STR, "VISIT": STR, "QSSTRESN": pl.Float64, "VISITNUM": pl.Float64,
    },
)
sched = pl.read_csv(
    INPUT / "schedule.csv",
    schema_overrides={"VISITNUM": pl.Float64, "VISITDY": pl.Float64,
                      "VISIT": STR, "PARCAT2": STR, "PARAMCD": STR, "PARAM": STR},
).with_row_index("_SCHORD")


def blank_to_null(df, cols):
    return df.with_columns(
        [pl.when(pl.col(c).str.strip_chars() == "").then(None).otherwise(pl.col(c)).alias(c)
         for c in cols]
    )


adsl = blank_to_null(adsl, ["SAFFL", "DCTREAS"])
qs = blank_to_null(qs, ["QSSTAT", "QSREASND", "QSTESTCD"])

# Study day of an event, counted from RANDDT as day 1.
adsl = adsl.with_columns(
    EOTDY=((pl.col("EOTDT") - pl.col("RANDDT")).dt.total_days() + 1).cast(pl.Float64),
    DTHDY=((pl.col("DTHDT") - pl.col("RANDDT")).dt.total_days() + 1).cast(pl.Float64),
)

# Skeleton: every ADSL subject x every SCHEDULE parameter/visit.
skel = adsl.select(
    "STUDYID", "USUBJID", "SAFFL", "DCTREAS", "EOTDY", "DTHDY"
).join(
    sched.select("_SCHORD", "VISITNUM", "VISIT", "VISITDY", "PARCAT2", "PARAMCD", "PARAM"),
    how="cross",
)

# Collected items: one QS record per subject/visit/testcd (first by QSSEQ if duplicated).
qs_items = (
    qs.filter(pl.col("QSTESTCD").is_in(ITEMS))
    .sort(["USUBJID", "VISITNUM", "QSTESTCD", "QSSEQ"])
    .unique(subset=["USUBJID", "VISITNUM", "QSTESTCD"], keep="first", maintain_order=True)
    .select(
        "USUBJID", "VISITNUM",
        pl.col("QSTESTCD").alias("PARAMCD"),
        "QSSTRESN", "QSSTAT", "QSREASND",
        pl.lit(True).alias("_COLL"),
    )
)

# Visit-level summary for the total score.
wide = skel.select("USUBJID", "VISITNUM").unique()
for item in ITEMS:
    sub = qs_items.filter(pl.col("PARAMCD") == item).select(
        "USUBJID", "VISITNUM",
        pl.col("QSSTRESN").alias(f"_{item}_N"),
        pl.col("QSREASND").alias(f"_{item}_R"),
        pl.col("_COLL").alias(f"_{item}_C"),
    )
    wide = wide.join(sub, on=["USUBJID", "VISITNUM"], how="left")

wide = wide.with_columns(
    [pl.col(f"_{i}_C").fill_null(False) for i in ITEMS]
).with_columns(
    _TOT_N=pl.when(pl.all_horizontal([pl.col(f"_{i}_N").is_not_null() for i in ITEMS]))
    .then(pl.sum_horizontal([pl.col(f"_{i}_N") for i in ITEMS]))
    .otherwise(None),
    _TOT_C=pl.any_horizontal([pl.col(f"_{i}_C") for i in ITEMS]),
    _TOT_R=pl.coalesce([pl.col(f"_{i}_R") for i in ITEMS]),
).select("USUBJID", "VISITNUM", "_TOT_N", "_TOT_C", "_TOT_R")

df = skel.join(qs_items, on=["USUBJID", "VISITNUM", "PARAMCD"], how="left").join(
    wide, on=["USUBJID", "VISITNUM"], how="left"
)

is_tot = pl.col("PARAMCD") == TOTAL
after_eot = pl.col("EOTDY").is_not_null() & (pl.col("VISITDY") > pl.col("EOTDY"))
after_dth = pl.col("DTHDY").is_not_null() & (pl.col("VISITDY") > pl.col("DTHDY"))

df = df.with_columns(
    _COLL=pl.when(is_tot).then(pl.col("_TOT_C")).otherwise(pl.col("_COLL").fill_null(False)),
    AVAL=pl.when(is_tot).then(pl.col("_TOT_N")).otherwise(pl.col("QSSTRESN")),
    _AFTER_EOT=after_eot,
    _AFTER_DTH=after_dth,
)
phantom = ~pl.col("_COLL")

df = df.with_columns(
    DTYPE=pl.when(phantom).then(pl.lit("PHANTOM")).otherwise(None),
    QSSTAT=pl.when(phantom | is_tot).then(None).otherwise(pl.col("QSSTAT")),
    QSREASND=pl.when(phantom | is_tot).then(None).otherwise(pl.col("QSREASND")),
    AREASND=pl.when(pl.col("AVAL").is_not_null()).then(None)
    .when(phantom & (pl.col("SAFFL") == "N")).then(pl.lit("NOT TREATED"))
    .when(phantom & pl.col("_AFTER_EOT")).then(pl.col("DCTREAS"))
    .when(phantom).then(None)
    .when(is_tot).then(pl.coalesce([pl.col("_TOT_R"), pl.lit("NOT CALCULABLE")]))
    .otherwise(pl.col("QSREASND")),
    PROEX1FL=pl.when(~pl.col("_AFTER_DTH")).then(pl.lit("Y")).otherwise(None),
    PROEX2FL=pl.when((pl.col("SAFFL") == "Y") & ~pl.col("_AFTER_EOT") & ~pl.col("_AFTER_DTH"))
    .then(pl.lit("Y")).otherwise(None),
    PROSCMFL=pl.when(pl.col("AVAL").is_not_null()).then(pl.lit("Y")).otherwise(None),
    AVISIT=pl.col("VISIT"),
    AVISITN=pl.col("VISITNUM"),
)

# Integer-valued numerics are written without a trailing ".0".
def as_int_if_whole(df, col):
    s = df[col]
    nn = s.drop_nulls()
    if nn.len() == 0 or (nn == nn.round(0)).all():
        return df.with_columns(pl.col(col).cast(pl.Int64))
    return df


df = df.sort(["USUBJID", "PARAMCD", "AVISITN"]).select(COLUMNS)
df = as_int_if_whole(df, "AVAL")
df = as_int_if_whole(df, "AVISITN")

OUTPUT.mkdir(parents=True, exist_ok=True)
df.write_csv(OUTPUT / "adqs.csv")
