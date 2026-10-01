# Reference solution for the yamaa benchmark adam-adae-query-flags (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
queries = pl.read_csv("/app/input/queries.csv", infer_schema=False).with_columns(
    pl.col("GRPID").cast(pl.Int64)
)

# Each standardized slot shows its grouping name, dictionary code, and
# scope together; the sponsor slot shows the grouping name only. Two
# events with the same coded term always show the same entries.
smq01 = queries.filter(pl.col("PREFIX") == "SMQ01").select(
    "TERM", SMQ01NAM="GRPNAME", SMQ01CD="GRPID", SMQ01SC="SCOPE"
)
smq02 = queries.filter(pl.col("PREFIX") == "SMQ02").select(
    "TERM", SMQ02NAM="GRPNAME", SMQ02CD="GRPID", SMQ02SC="SCOPE"
)
cq01 = queries.filter(pl.col("PREFIX") == "CQ01").select("TERM", CQ01NAM="GRPNAME")

adae = (
    ae.join(smq01, left_on="AEDECOD", right_on="TERM", how="left", maintain_order="left")
    .join(smq02, left_on="AEDECOD", right_on="TERM", how="left", maintain_order="left")
    .join(cq01, left_on="AEDECOD", right_on="TERM", how="left", maintain_order="left")
    .select(
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "AEDECOD",
        "SMQ01NAM",
        "SMQ01CD",
        "SMQ01SC",
        "SMQ02NAM",
        "SMQ02CD",
        "SMQ02SC",
        "CQ01NAM",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
