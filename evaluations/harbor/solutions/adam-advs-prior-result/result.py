# Reference solution for the yamaa benchmark adam-advs-prior-result (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

vs = pl.read_csv("/app/input/vs.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64, strict=False),
    pl.col("AVISITN").cast(pl.Int64, strict=False),
)

# Rows with no series value share one series within each subject; order
# within a series by visit number with missing numbers last, keeping the
# collected order (VSSEQ) for ties.
ordered = vs.sort(
    ["STUDYID", "USUBJID", "SERIES", "AVISITN", "VSSEQ"],
    nulls_last=True,
)

# The closest earlier result with a value in the same subject and series.
prev_by_key: dict[tuple, str | None] = {}
last_seen: dict[tuple, str | None] = {}
for record in ordered.iter_rows(named=True):
    key = (record["STUDYID"], record["USUBJID"], record["SERIES"])
    prev_by_key[(record["STUDYID"], record["USUBJID"], record["VSSEQ"])] = last_seen.get(key)
    if record["AVALC"] is not None:
        last_seen[key] = record["AVALC"]

advs = (
    vs.with_columns(
        PREVAVALC=pl.struct(["STUDYID", "USUBJID", "VSSEQ"]).map_elements(
            lambda s: prev_by_key.get((s["STUDYID"], s["USUBJID"], s["VSSEQ"])),
            return_dtype=pl.String,
        )
    )
    .sort(["USUBJID", "VSSEQ"])
    .select("STUDYID", "USUBJID", "VSSEQ", "SERIES", "AVISITN", "AVALC", "PREVAVALC")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
advs.write_csv("/app/output/advs.csv")
