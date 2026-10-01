# Reference solution for the yamaa benchmark adam-adae-serious-sequence (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

# Number the subject's serious events in onset order, so the earliest
# carries 1. Events sharing one onset date follow collection order, and
# a serious event with no onset date is numbered last. ISO dates sort
# as text. Numbering restarts for each subject.
numbered = (
    ae.filter(pl.col("AESER") == "Y")
    .with_columns(ASTDT=pl.col("AESTDTC"))
    .sort(["USUBJID", "AESTDTC", "AESEQ"], nulls_last=True)
    .with_columns(SERSEQ=pl.col("AESEQ").cum_count().over("USUBJID"))
    .select("USUBJID", "AESEQ", "SERSEQ")
)

adae = (
    ae.with_columns(ASTDT=pl.col("AESTDTC"))
    .join(numbered, on=["USUBJID", "AESEQ"], how="left", maintain_order="left")
    .select("STUDYID", "USUBJID", "AESEQ", "AESER", "ASTDT", "SERSEQ")
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
