# Reference solution for the yamaa benchmark adam-adtr-nadir (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/adtr_raw.csv", infer_schema=False, null_values="").with_columns(
    pl.col("AVISITN").cast(pl.Int64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
    pl.col("AVAL").cast(pl.Float64, strict=False),
    pl.col("NMEAS").cast(pl.Int64, strict=False),
    pl.col("NTARGET").cast(pl.Int64, strict=False),
)

# Complete dated assessments carry the nadir candidates; a complete
# assessment with no date never counts toward any nadir, its own
# included. An incomplete current assessment keeps the nadir set by an
# earlier complete one, while a current record with no date has none.
complete = [
    (r["USUBJID"], r["ADT"], r["AVAL"])
    for r in raw.to_dicts()
    if r["ANL01FL"] == "Y" and r["ADT"] is not None and r["AVAL"] is not None
]

nadirs = []
for r in raw.to_dicts():
    if r["ADT"] is None:
        nadirs.append(None)
        continue
    cands = [v for (u, d, v) in complete if u == r["USUBJID"] and d <= r["ADT"]]
    nadirs.append(min(cands) if cands else None)

adtr = (
    raw.with_columns(NADIR=pl.Series(nadirs, dtype=pl.Float64))
    .select(
        "STUDYID",
        "USUBJID",
        "AVISIT",
        "AVISITN",
        "ADT",
        "PARAMCD",
        "PARAM",
        "AVAL",
        "NMEAS",
        "NTARGET",
        "ANL01FL",
        "NADIR",
    )
    .sort(["USUBJID", "AVISITN"])
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adtr.write_csv("/app/output/adtr.csv", null_value="")
