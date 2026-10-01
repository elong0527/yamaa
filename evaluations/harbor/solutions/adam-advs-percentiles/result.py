# Reference solution for the yamaa benchmark adam-advs-percentiles (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import math
from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/advs_raw.csv", infer_schema=False).with_columns(
    pl.col("AVAL").cast(pl.Float64, strict=False),
    pl.col("AGE").cast(pl.Int64, strict=False),
)
ref = pl.read_csv("/app/input/lms_ref.csv", infer_schema=False).with_columns(
    pl.col("AGE").cast(pl.Int64, strict=False),
    pl.col("L").cast(pl.Float64, strict=False),
    pl.col("M").cast(pl.Float64, strict=False),
    pl.col("S").cast(pl.Float64, strict=False),
)


def percentile(aval: float | None, ell: float | None, emm: float | None, ess: float | None) -> float | None:
    if aval is None or ell is None or emm is None or ess is None:
        return None
    if emm == 0 or ess == 0:
        return None
    if ell == 0:
        z = math.log(aval / emm) / ess
    else:
        z = ((aval / emm) ** ell - 1) / (ell * ess)
    return 100 * 0.5 * (1 + math.erf(z / math.sqrt(2)))


joined = raw.join(ref, on=["PARAMCD", "SEX", "AGE"], how="left")

rows = []
for record in joined.sort(["USUBJID", "AVISIT", "PARAMCD"]).iter_rows(named=True):
    if record["PARAMCD"] == "BMI":
        out_code, out_label = "BMIPCTL", "BMI-for-Age Percentile"
    else:
        out_code, out_label = "WGTPCTL", "Weight-for-Age Percentile"
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "AVISIT": record["AVISIT"],
            "PARAMCD": out_code,
            "PARAM": out_label,
            "AVAL": percentile(record["AVAL"], record["L"], record["M"], record["S"]),
        }
    )

advs = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "AVISIT": pl.String,
        "PARAMCD": pl.String,
        "PARAM": pl.String,
        "AVAL": pl.Float64,
    },
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
