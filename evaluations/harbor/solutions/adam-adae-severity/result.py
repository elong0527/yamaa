# Reference solution for the yamaa benchmark adam-adae-severity (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
supp = pl.read_csv("/app/input/supp.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
).select("STUDYID", "USUBJID", "AESEQ", "AESEV")

# Severity is matched on the study and subject identifiers together with
# the event sequence number. An event with no supplemental record, or a
# record carrying no severity, has no value.
adae = ae.join(
    supp, on=["STUDYID", "USUBJID", "AESEQ"], how="left", maintain_order="left"
).select("STUDYID", "USUBJID", "AESEQ", "AEDECOD", "AESEV")

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
