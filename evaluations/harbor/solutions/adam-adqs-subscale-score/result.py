# Reference solution for the yamaa benchmark adam-adqs-subscale-score (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

qs = pl.read_csv("/app/input/qs.csv", infer_schema=False).with_columns(
    pl.col("QSSEQ").cast(pl.Int64),
    pl.col("QSSTRESN").cast(pl.Float64),
)

# Each physical functioning item response, on its zero to four scale.
items = (
    qs.filter(pl.col("QSTESTCD").is_in(["PF01", "PF02", "PF03", "PF04"]))
    .with_columns(
        PARAMCD=pl.col("QSTESTCD"), PARAM=pl.col("QSTEST"), AVAL=pl.col("QSSTRESN")
    )
    .select("STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL")
)

# One score record per visit with a PF01 item record: the mean of the
# answered items on a zero to one hundred scale, with no value when fewer
# than three of the four items were answered.
visits = items.filter(pl.col("PARAMCD") == "PF01").select(
    "STUDYID", "USUBJID", "VISIT"
).unique(maintain_order=True)

stats = items.group_by(["STUDYID", "USUBJID", "VISIT"]).agg(
    n_answered=pl.col("AVAL").drop_nulls().len(),
    mean_answered=pl.col("AVAL").mean(),
)

scores = (
    visits.join(stats, on=["STUDYID", "USUBJID", "VISIT"], how="left")
    .with_columns(
        AVAL=pl.when(pl.col("n_answered") >= 3)
        .then(pl.col("mean_answered") * 25)
        .otherwise(None)
    )
    .with_columns(
        PARAMCD=pl.lit("PFSCORE"),
        PARAM=pl.lit("Physical Functioning Subscale Score"),
    )
    .select("STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL")
)

adqs = pl.concat([items, scores], how="diagonal").sort(
    ["STUDYID", "USUBJID", "VISIT", "PARAMCD"]
)

Path("/app/output").mkdir(exist_ok=True)
adqs.write_csv("/app/output/adqs.csv")
