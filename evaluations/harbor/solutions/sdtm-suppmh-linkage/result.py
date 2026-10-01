# Reference solution for the yamaa benchmark sdtm-suppmh-linkage (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

mh = pl.read_csv("/app/input/mh.csv", infer_schema=False)
raw = pl.read_csv("/app/input/mh_supp_raw.csv", infer_schema=False)

labels = {"MHFAMHX": "Family History", "MHCONF": "Confirmed by Medical Records"}

# The parent sequence number, matched on subject and condition term.
parents = mh.select("STUDYID", "USUBJID", "MHSEQ", "MHTERM")

records = []
for qnam in ("MHFAMHX", "MHCONF"):
    collected = (
        raw.filter(pl.col(qnam).is_not_null() & (pl.col(qnam) != ""))
        .join(parents, on=["STUDYID", "USUBJID", "MHTERM"], how="inner")
        .select(
            "STUDYID",
            "USUBJID",
            "MHSEQ",
            QNAM=pl.lit(qnam),
            QLABEL=pl.lit(labels[qnam]),
            QVAL=pl.col(qnam),
        )
    )
    records.append(collected)

suppmh = (
    pl.concat(records)
    .with_columns(
        RDOMAIN=pl.lit("MH"),
        IDVAR=pl.lit("MHSEQ"),
        IDVARVAL=pl.col("MHSEQ"),
        QORIG=pl.lit("Collected"),
        QEVAL=pl.lit(None, dtype=pl.String),
    )
    .with_columns(pl.col("MHSEQ").cast(pl.Int64).alias("_seq"))
    .sort(["USUBJID", "_seq", "QNAM"])
    .select(
        "STUDYID",
        "RDOMAIN",
        "USUBJID",
        "IDVAR",
        "IDVARVAL",
        "QNAM",
        "QLABEL",
        "QVAL",
        "QORIG",
        "QEVAL",
    )
)

Path("/app/output").mkdir(exist_ok=True)
suppmh.write_csv("/app/output/suppmh.csv")
