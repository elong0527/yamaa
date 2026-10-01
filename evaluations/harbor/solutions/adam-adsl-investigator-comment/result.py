# Reference solution for the yamaa benchmark adam-adsl-investigator-comment (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False, null_values="")

# An empty comment field, quoted or not, counts as no comment collected,
# while a field holding only spaces is still a comment and the spaces
# are kept. Reading with null_values="" turns both empty forms into
# null and keeps spaces as-is.
adsl = (
    dm.with_columns(
        CMNT=pl.col("COMMENT"),
        CMNTFL=pl.when(pl.col("COMMENT").is_null())
        .then(pl.lit("N"))
        .otherwise(pl.lit("Y")),
    )
    .select("STUDYID", "USUBJID", "CMNT", "CMNTFL")
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv", null_value="")
