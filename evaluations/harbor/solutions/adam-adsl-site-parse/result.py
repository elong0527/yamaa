# Reference solution for the yamaa benchmark adam-adsl-site-parse (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False, null_values="")

adsl = (
    dm.with_columns(
        # The middle segment when the identifier holds two dashes with
        # exactly four digits after the last one.
        SITEIDP=pl.col("USUBJID").str.extract(r"^YAMAA-([^-]+)-[0-9]{4}$", 1),
    )
    .with_columns(
        SITEID=pl.coalesce("SITEIDP", "SITEID").fill_null("UNKNOWN"),
    )
    .with_columns(
        SUBJREF=pl.when(pl.col("SUBJID").is_null())
        .then(pl.lit("UNKNOWN"))
        .otherwise(pl.col("SITEID") + pl.lit(":") + pl.col("SUBJID")),
    )
    .select("STUDYID", "USUBJID", "SUBJID", "SITEIDP", "SITEID", "SUBJREF")
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv", null_value="")
