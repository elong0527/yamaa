# Reference solution for the yamaa benchmark adam-adpp-comparison-sets (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/adpp_raw.csv", infer_schema=False).with_columns(
    pl.col("APERIOD").cast(pl.Int64),
    pl.col("AVAL").cast(pl.Float64, strict=False),
)
adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).select(
    "STUDYID", "USUBJID", "TRTSEQP"
)
compare = pl.read_csv("/app/input/compare.csv", infer_schema=False).with_columns(
    pl.col("COMPNO").cast(pl.Int64)
)

keys = ["STUDYID", "USUBJID", "PARAMCD"]
if raw.select(keys + ["TRTA"]).is_duplicated().any():
    raise SystemExit("more than one record per subject, parameter and treatment")

adpp = raw.join(adsl, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")

# Flag nn belongs to comparison nn. A record is used when its treatment is
# one of the two, its own value is evaluable (present and > 0), and the same
# subject has an evaluable value of the same parameter under the other one.
for number in (1, 2):
    row = compare.filter(pl.col("COMPNO") == number)
    if row.height != 1:
        raise SystemExit(f"comparison {number} must be listed once")
    test, ref = row["TESTTRT"][0], row["REFTRT"][0]
    partner = raw.select(
        *keys,
        pl.col("TRTA").alias("_OTHER"),
        pl.col("AVAL").alias("_PAVAL"),
    )
    flag = f"ANL{number:02d}FL"
    adpp = (
        adpp.with_columns(
            _OTHER=pl.when(pl.col("TRTA") == test)
            .then(pl.lit(ref))
            .when(pl.col("TRTA") == ref)
            .then(pl.lit(test))
        )
        .join(partner, on=keys + ["_OTHER"], how="left", maintain_order="left")
        .with_columns(
            pl.when(
                pl.col("_OTHER").is_not_null()
                & (pl.col("AVAL") > 0).fill_null(False)
                & (pl.col("_PAVAL") > 0).fill_null(False)
            )
            .then(pl.lit("Y"))
            .otherwise(pl.lit(None, dtype=pl.String))
            .alias(flag)
        )
        .drop("_OTHER", "_PAVAL")
    )

adpp = adpp.select(
    "STUDYID",
    "USUBJID",
    "PARAMCD",
    "PARAM",
    "APERIOD",
    "TRTA",
    "TRTSEQP",
    "AVAL",
    "ANL01FL",
    "ANL02FL",
)

Path("/app/output").mkdir(exist_ok=True)
adpp.write_csv("/app/output/adpp.csv")
