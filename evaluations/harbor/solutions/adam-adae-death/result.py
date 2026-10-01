# Reference solution for the yamaa benchmark adam-adae-death (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)

# The fatal adverse event behind each death: the most recent one, and the
# highest AESEQ among those starting that day. ISO dates sort as text.
fatal = (
    ae.filter(pl.col("AEOUT") == "FATAL")
    .sort(["ASTDT", "AESEQ"], descending=True, nulls_last=True)
    .unique("USUBJID", keep="first", maintain_order=True)
    .select("USUBJID", DTHCAUS="AEDECOD", FATALDT="ASTDT")
)

# One record per subject who died, from a fatal adverse event, DM, or both.
death = (
    dm.filter(pl.col("DTHDT").is_not_null())
    .select("USUBJID", DMDTHDT="DTHDT")
    .join(fatal, on="USUBJID", how="full", coalesce=True)
    .with_columns(DTHFL=pl.lit("Y"), DTHDT=pl.coalesce("FATALDT", "DMDTHDT"))
    .select("USUBJID", "DTHFL", "DTHCAUS", "DTHDT")
)

adae = ae.join(death, on="USUBJID", how="left", maintain_order="left").select(
    "STUDYID", "USUBJID", "AESEQ", "AEDECOD", "ASTDT", "DTHFL", "DTHCAUS", "DTHDT"
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
