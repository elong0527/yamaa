# Reference solution for the yamaa benchmark
# adam-advs-orthostatic-criteria (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("VSSTRESN").cast(pl.Float64, strict=False)
)


def reading(test: str, position: str, time_point: str | None = None):
    """The one reading of a test in a position at a visit, if any."""
    mask = (pl.col("VSTESTCD") == test) & (pl.col("VSPOS") == position)
    if time_point is not None:
        mask = mask & (pl.col("VSTPT") == time_point)
    return pl.col("VSSTRESN").filter(mask).first()


standing = "AFTER 3 MIN STANDING"
visits = (
    vs.group_by("STUDYID", "USUBJID", "VISIT", maintain_order=True)
    .agg(
        SYSBP_SUP=reading("SYSBP", "SUPINE"),
        SYSBP_STD=reading("SYSBP", "STANDING", standing),
        DIABP_SUP=reading("DIABP", "SUPINE"),
        DIABP_STD=reading("DIABP", "STANDING", standing),
        HR_SUP=reading("HR", "SUPINE"),
        HR_STD=reading("HR", "STANDING", standing),
    )
    .rename({"VISIT": "AVISIT"})
    .with_columns(
        SYSBP_CHG=pl.col("SYSBP_STD") - pl.col("SYSBP_SUP"),
        DIABP_CHG=pl.col("DIABP_STD") - pl.col("DIABP_SUP"),
        HR_CHG=pl.col("HR_STD") - pl.col("HR_SUP"),
    )
)

keys = ["STUDYID", "USUBJID", "AVISIT"]


def change(paramcd: str, param: str, column: str) -> pl.DataFrame:
    return visits.filter(pl.col(column).is_not_null()).select(
        *keys,
        PARAMCD=pl.lit(paramcd),
        PARAM=pl.lit(param),
        AVAL=pl.col(column),
        AVALC=pl.lit(None, dtype=pl.String),
    )


# Either blood-pressure fall is enough for Y, even when the other
# change is missing; N needs both changes, so a visit with neither
# fall and one change missing gets no record.
hypotension = (
    visits.with_columns(
        AVALC=pl.when(
            (pl.col("SYSBP_CHG") <= -20).fill_null(False)
            | (pl.col("DIABP_CHG") <= -10).fill_null(False)
        )
        .then(pl.lit("Y"))
        .when(
            pl.col("SYSBP_CHG").is_not_null()
            & pl.col("DIABP_CHG").is_not_null()
        )
        .then(pl.lit("N"))
        .otherwise(None)
    )
    .filter(pl.col("AVALC").is_not_null())
    .select(
        *keys,
        PARAMCD=pl.lit("OHYPO"),
        PARAM=pl.lit("Orthostatic Hypotension"),
        AVAL=pl.lit(None, dtype=pl.Float64),
        AVALC=pl.col("AVALC"),
    )
)

tachycardia = (
    visits.with_columns(
        AVALC=pl.when(pl.col("HR_CHG").is_null())
        .then(None)
        .when(pl.col("HR_CHG") >= 30)
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N"))
    )
    .filter(pl.col("AVALC").is_not_null())
    .select(
        *keys,
        PARAMCD=pl.lit("OTACHY"),
        PARAM=pl.lit("Orthostatic Tachycardia"),
        AVAL=pl.lit(None, dtype=pl.Float64),
        AVALC=pl.col("AVALC"),
    )
)

advs = pl.concat(
    [
        change(
            "SYSBPOC",
            "Systolic Blood Pressure Change on Standing (mmHg)",
            "SYSBP_CHG",
        ),
        change(
            "DIABPOC",
            "Diastolic Blood Pressure Change on Standing (mmHg)",
            "DIABP_CHG",
        ),
        change("HROC", "Heart Rate Change on Standing (beats/min)", "HR_CHG"),
        hypotension,
        tachycardia,
    ]
).select("STUDYID", "USUBJID", "AVISIT", "PARAMCD", "PARAM", "AVAL", "AVALC")

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
