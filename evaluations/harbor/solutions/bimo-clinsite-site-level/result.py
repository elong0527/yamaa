# Reference solution for the yamaa benchmark bimo-clinsite-site-level (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

subj = pl.read_csv("/app/input/adsl.csv", infer_schema=False, null_values="")

clinsite = (
    # One record per site; the site key stays text so leading zeros survive.
    subj.group_by("STUDYID", "SITENUM", maintain_order=True)
    .agg(
        # The alphabetically first arm among the site's treated subjects.
        pl.col("ARM").filter(pl.col("SAFFL") == "Y").min().alias("ARM"),
        # Treated subjects, then all enrolled subjects, at the site.
        (pl.col("SAFFL") == "Y").sum().alias("SAFPOP"),
        pl.len().alias("ENRLPOP"),
    )
    .rename({"SITENUM": "SITEID"})
    .select("STUDYID", "SITEID", "ARM", "SAFPOP", "ENRLPOP")
    .sort("SITEID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
clinsite.write_csv("/app/output/clinsite.csv", null_value="")
