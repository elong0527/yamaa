# Reference solution for the yamaa benchmark adam-adqs-multi-scale-scoring (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

qs = pl.read_csv("/app/input/qs.csv", infer_schema=False).with_columns(
    pl.col("QSSEQ").cast(pl.Int64),
    pl.col("QSSTRESN").cast(pl.Float64),
)

# Each item response, on its own answer scale.
items = qs.with_columns(
    PARAMCD=pl.col("QSTESTCD"), PARAM=pl.col("QSTEST"), AVAL=pl.col("QSSTRESN")
).select("STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL")

# The four scales: member items, anchor item, minimum answered items,
# direction (down = higher score means better, up = higher means worse),
# and the answer range.
scales = [
    (
        "F1SCORE",
        "Physical Functioning Scale Score",
        ["F101", "F102", "F103", "F104"],
        "F101",
        2,
        "down",
        3,
    ),
    ("F2SCORE", "Role Functioning Scale Score", ["F201", "F202"], "F201", 1, "down", 3),
    (
        "SSCORE",
        "Fatigue and Sleep Symptom Scale Score",
        ["S01", "S02", "F104"],
        "S01",
        2,
        "up",
        3,
    ),
    ("GSCORE", "Global Health Scale Score", ["G01", "G02"], "G01", 1, "up", 6),
]

score_frames = []
for pcode, pname, members, anchor, min_ans, direction, rng in scales:
    member_rows = items.filter(pl.col("PARAMCD").is_in(members))
    visits = (
        items.filter(pl.col("PARAMCD") == anchor)
        .select("STUDYID", "USUBJID", "VISIT")
        .unique(maintain_order=True)
    )
    stats = member_rows.group_by(["STUDYID", "USUBJID", "VISIT"]).agg(
        n_answered=pl.col("AVAL").drop_nulls().len(),
        raw_mean=pl.col("AVAL").mean(),
    )
    if direction == "down":
        value = 100 * (1 - (pl.col("raw_mean") - 1) / rng)
    else:
        value = 100 * (pl.col("raw_mean") - 1) / rng
    scores = (
        visits.join(stats, on=["STUDYID", "USUBJID", "VISIT"], how="left")
        .with_columns(
            AVAL=pl.when(pl.col("n_answered") >= min_ans).then(value).otherwise(None)
        )
        .with_columns(PARAMCD=pl.lit(pcode), PARAM=pl.lit(pname))
        .select("STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL")
    )
    score_frames.append(scores)

adqs = pl.concat([items, *score_frames], how="diagonal").sort(
    ["STUDYID", "USUBJID", "VISIT", "PARAMCD"]
)

Path("/app/output").mkdir(exist_ok=True)
adqs.write_csv("/app/output/adqs.csv")
