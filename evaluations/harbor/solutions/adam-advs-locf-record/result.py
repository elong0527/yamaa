# Reference solution for the yamaa benchmark adam-advs-locf-record (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

plan = pl.read_csv("/app/input/plan.csv", infer_schema=False).with_columns(
    pl.col("AVISITN").cast(pl.Int64, strict=False)
)
vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("AVISITN").cast(pl.Int64, strict=False),
    pl.col("AVAL").cast(pl.Float64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
    pl.col("QSSEQ").cast(pl.Int64, strict=False),
)

# Only results selected for analysis can be carried.
candidates = vs.filter(
    (pl.col("ANL01FL") == "Y") & pl.col("AVAL").is_not_null()
)

# The latest non-missing selected result at or before the planned visit:
# the highest visit number, then the highest sequence number.
rows = []
for planned in plan.sort(["USUBJID", "PARAMCD", "AVISITN"]).iter_rows(named=True):
    donor = (
        candidates.filter(
            (pl.col("USUBJID") == planned["USUBJID"])
            & (pl.col("PARAMCD") == planned["PARAMCD"])
            & (pl.col("AVISITN") <= planned["AVISITN"])
        )
        .sort(["AVISITN", "QSSEQ"])
        .tail(1)
    )
    if donor.height:
        record = donor.row(0, named=True)
        rows.append(
            {
                "USUBJID": planned["USUBJID"],
                "PARAMCD": planned["PARAMCD"],
                "AVISITN": planned["AVISITN"],
                "AVAL": record["AVAL"],
                "ADT": record["ADT"],
                "QSSEQ": record["QSSEQ"],
            }
        )
    else:
        rows.append(
            {
                "USUBJID": planned["USUBJID"],
                "PARAMCD": planned["PARAMCD"],
                "AVISITN": planned["AVISITN"],
                "AVAL": None,
                "ADT": None,
                "QSSEQ": None,
            }
        )

advs = pl.DataFrame(
    rows,
    schema={
        "USUBJID": pl.String,
        "PARAMCD": pl.String,
        "AVISITN": pl.Int64,
        "AVAL": pl.Float64,
        "ADT": pl.Date,
        "QSSEQ": pl.Int64,
    },
).sort(["USUBJID", "PARAMCD", "AVISITN"])

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
