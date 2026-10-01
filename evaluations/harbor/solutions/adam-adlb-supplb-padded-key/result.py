# Reference solution for the yamaa benchmark adam-adlb-supplb-padded-key (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

lb = pl.read_csv("/app/input/lb.csv", infer_schema=False).with_columns(
    pl.col("LBSEQ").cast(pl.Int64, strict=False)
)
supp = pl.read_csv("/app/input/supplb.csv", infer_schema=False)

# The qualifier value whose IDVARVAL is the laboratory sequence number as
# eight characters, right-aligned and space-padded. Zero-padded keys do
# not match; a supplemental record with no laboratory record adds no row.
lb_keys = lb.with_columns(
    PADDED_KEY=pl.col("LBSEQ").cast(pl.String).str.pad_start(8, " ")
)

adlb = (
    lb_keys.join(
        supp.select("STUDYID", "USUBJID", "IDVARVAL", QVAL="QVAL"),
        left_on=["STUDYID", "USUBJID", "PADDED_KEY"],
        right_on=["STUDYID", "USUBJID", "IDVARVAL"],
        how="left",
        maintain_order="left",
    )
    .with_columns(QVAL_NAMED=pl.col("QVAL"))
    .select("STUDYID", "USUBJID", "LBSEQ", "QVAL", "QVAL_NAMED")
    .sort(["LBSEQ", "USUBJID"])
)

Path("/app/output").mkdir(exist_ok=True)
adlb.write_csv("/app/output/adlb.csv")
