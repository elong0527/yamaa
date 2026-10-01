# Reference solution for the yamaa benchmark sdtm-lb-ranges (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value) != ""


lb_raw = pl.read_csv("/app/input/lb_raw.csv", infer_schema=False)
dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
lbrange = pl.read_csv("/app/input/lbrange.csv", infer_schema=False)

base = lb_raw.join(dm.select("USUBJID", "SEX"), on="USUBJID", how="left")
joined = base.join(lbrange, on=["LBTESTCD", "SEX"], how="left")


def _indicator(row) -> str | None:
    result, lo, hi = row["LBSTRESN"], row["NRLO"], row["NRHI"]
    if not _present(result) or not _present(lo) or not _present(hi):
        return None
    try:
        value, low, high = float(result), float(lo), float(hi)
    except ValueError:
        return None
    if value < low:
        return "LOW"
    if value > high:
        return "HIGH"
    return "NORMAL"


rows = []
for row in joined.to_dicts():
    rows.append(
        {
            "DOMAIN": "LB",
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "LBSEQ": row["LBSEQ"],
            "LBTESTCD": row["LBTESTCD"],
            "LBSTRESN": row["LBSTRESN"] if _present(row["LBSTRESN"]) else None,
            "LBSTRESU": row["UNIT"] if _present(row.get("UNIT")) else None,
            "LBSTNRLO": row["NRLO"] if _present(row.get("NRLO")) else None,
            "LBSTNRHI": row["NRHI"] if _present(row.get("NRHI")) else None,
            "LBNRIND": _indicator(row),
        }
    )

lb = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "LBSEQ": pl.String,
            "LBTESTCD": pl.String,
            "LBSTRESN": pl.String,
            "LBSTRESU": pl.String,
            "LBSTNRLO": pl.String,
            "LBSTNRHI": pl.String,
            "LBNRIND": pl.String,
        },
    )
    .with_columns(pl.col("LBSEQ").cast(pl.Int64).alias("_seq"))
    .sort(["USUBJID", "_seq"])
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "LBSTRESN",
        "LBSTRESU",
        "LBSTNRLO",
        "LBSTNRHI",
        "LBNRIND",
    )
)

Path("/app/output").mkdir(exist_ok=True)
lb.write_csv("/app/output/lb.csv")
