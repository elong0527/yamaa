# Reference solution for the yamaa benchmark sdtm-tr-tumor-measurements (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def fmt_num(x: float | None) -> str | None:
    import math

    if x is None:
        return None
    if isinstance(x, float) and math.isnan(x):
        return None
    if float(x).is_integer():
        return str(int(float(x)))
    text = repr(float(x))
    if "e" in text or "E" in text:
        text = f"{float(x):.15f}".rstrip("0").rstrip(".")
    else:
        text = text.rstrip("0").rstrip(".") if "." in text else text
    return text


raw = pl.read_csv("/app/input/tr_raw.csv", infer_schema=False).with_columns(
    pl.col("TRSEQ").cast(pl.Int64),
    pl.col("VISITNUM").cast(pl.Int64),
)

is_diameter = pl.col("TRTESTCD") == "DIAMETER"
is_too_small = pl.col("TRORRES") == "TOO SMALL TO MEASURE"
is_missing = pl.col("TRORRES").is_null() | (pl.col("TRORRES") == "")

orres_num = pl.col("TRORRES").cast(pl.Float64, strict=False)
mm_value = (
    pl.when(pl.col("TRORRESU") == "cm")
    .then(orres_num * 10)
    .otherwise(orres_num)
)

stresn = (
    pl.when(is_diameter & is_too_small)
    .then(pl.lit(5.0))
    .when(is_diameter & ~is_missing & ~is_too_small)
    .then(mm_value)
    .otherwise(pl.lit(None, dtype=pl.Float64))
)

tr = raw.with_columns(TRSTRESN=stresn)

# Standardized text: 5 for too small, the lesion state as collected, the
# diameter in mm without a decimal point for whole numbers, else no value.
rows = tr.to_dicts()
for row in rows:
    if row["TRTESTCD"] == "DIAMETER":
        if row["TRORRES"] == "TOO SMALL TO MEASURE":
            row["TRSTRESC"] = "5"
        elif row["TRORRES"] is None or row["TRORRES"] == "":
            row["TRSTRESC"] = None
        else:
            row["TRSTRESC"] = fmt_num(row["TRSTRESN"])
    else:
        if row["TRORRES"] is None or row["TRORRES"] == "":
            row["TRSTRESC"] = None
        else:
            row["TRSTRESC"] = row["TRORRES"]

tr = (
    pl.DataFrame(rows)
    .with_columns(
        DOMAIN=pl.lit("TR"),
        TRSTRESU=pl.when(pl.col("TRSTRESN").is_not_null())
        .then(pl.lit("mm"))
        .otherwise(pl.lit(None, dtype=pl.String)),
    )
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "TRSEQ",
        "TRLNKID",
        "TRTESTCD",
        "TRTEST",
        "TRORRES",
        "TRORRESU",
        "TRSTRESC",
        "TRSTRESN",
        "TRSTRESU",
        "TRSTAT",
        "TRMETHOD",
        "TREVAL",
        "VISITNUM",
        "TRDTC",
    )
    .sort(["USUBJID", "TRSEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
tr.write_csv("/app/output/tr.csv")
