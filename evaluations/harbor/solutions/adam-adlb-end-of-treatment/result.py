# Reference solution for the yamaa benchmark adam-adlb-end-of-treatment (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("LBSEQ").cast(pl.Int64),
    pl.col("VISITNUM").cast(pl.Int64, strict=False),
    pl.col("LBSTRESN").cast(pl.Float64, strict=False).alias("AVAL"),
)
supp = pl.read_csv("/app/input/supplb.csv", infer_schema=False)

# ENDPOINT is Y when a supplemental endpoint qualifier with the same
# study, subject, and sequence number marks the record. Only the protocol
# endpoint qualifier is read. IDVARVAL is space-padded, so compare on the
# trimmed sequence number.
endpoint_keys = (
    supp.filter(pl.col("QNAM") == "ENDPOINT")
    .with_columns(pl.col("IDVARVAL").str.strip_chars().alias("SEQ_TRIM"))
    .select("STUDYID", "USUBJID", "SEQ_TRIM")
    .with_columns(pl.lit("Y").alias("ENDPOINT"))
)

marked = (
    lb.with_columns(SEQ_TRIM=pl.col("LBSEQ").cast(pl.String))
    .join(
        endpoint_keys,
        on=["STUDYID", "USUBJID", "SEQ_TRIM"],
        how="left",
        maintain_order="left",
    )
    .drop("SEQ_TRIM")
)

# EOTFL is Y on one record per subject and test: the endpoint-marked
# record when there is one, otherwise the record from the latest visit.
endpoint_winners = (
    marked.filter(pl.col("ENDPOINT") == "Y")
    .group_by("STUDYID", "USUBJID", "LBTESTCD")
    .agg(pl.col("LBSEQ").min().alias("WIN_ENDPOINT"))
)
latest_winners = (
    marked.sort(["VISITNUM", "LBSEQ"], descending=[True, True], nulls_last=True)
    .unique(["STUDYID", "USUBJID", "LBTESTCD"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", "LBTESTCD", WIN_LATEST="LBSEQ")
)
winners = (
    latest_winners.join(
        endpoint_winners, on=["STUDYID", "USUBJID", "LBTESTCD"], how="left"
    )
    .with_columns(WIN_LBSEQ=pl.coalesce("WIN_ENDPOINT", "WIN_LATEST"))
    .select("STUDYID", "USUBJID", "LBTESTCD", "WIN_LBSEQ")
    .with_columns(pl.lit("Y").alias("EOTFL"))
)

adlb = (
    marked.join(
        winners,
        left_on=["STUDYID", "USUBJID", "LBTESTCD", "LBSEQ"],
        right_on=["STUDYID", "USUBJID", "LBTESTCD", "WIN_LBSEQ"],
        how="left",
    )
    .select(
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "VISITNUM",
        "AVAL",
        "ENDPOINT",
        "EOTFL",
    )
    .sort(["USUBJID", "LBTESTCD", "LBSEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
