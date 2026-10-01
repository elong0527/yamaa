# Reference solution for the yamaa benchmark adam-adsl-rescue-med (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False, null_values="")
cm = pl.read_csv("/app/input/cm.csv", infer_schema=False, null_values="").with_columns(
    pl.col("CMSEQ").cast(pl.Int64, strict=False),
)

# The first rescue medication by earliest start date, with the smaller
# sequence number breaking a tie. A start holding only year and month
# compares as written, so it falls before any full date in the same
# month; a record with no start date still counts. Comparing the
# collected text with empty sorting first keeps both rules.
first = (
    cm.filter(pl.col("CMCAT") == "RESCUE MEDICATION")
    .with_columns(__sortkey=pl.col("CMSTDTC").fill_null(""))
    .sort(["__sortkey", "CMSEQ"], nulls_last=False)
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", RESCTRT="CMTRT")
)

adsl = (
    dm.join(first, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .select("STUDYID", "USUBJID", "RESCTRT")
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv", null_value="")
