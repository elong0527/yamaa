# Reference solution for the yamaa benchmark adam-adrs-best-response (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False).with_columns(
    pl.col("RANDDT").str.to_date()
)
selection = pl.read_csv(
    "/app/input/adrs_selection.csv", infer_schema=False
).with_columns(
    pl.col("ASEQ").cast(pl.Int64),
    pl.col("ADT").str.to_date(),
    pl.col("RANDDY").cast(pl.Int64),
    pl.col("BORPRI").cast(pl.Int64),
    pl.col("BORSEQ").cast(pl.Int64),
)

# The best overall response is the selection record numbered 1.
best = selection.filter(pl.col("BORSEQ") == 1).select(
    "USUBJID", BORCAT="BORCAT", BESTDT="ADT"
)

adrs = (
    adsl.join(best, on="USUBJID", how="left")
    .with_columns(
        PARAMCD=pl.lit("BOR"),
        PARAM=pl.lit("Best Overall Response by Investigator"),
        AVALC=pl.col("BORCAT"),
        AVAL=pl.col("BORCAT").replace_strict(
            {"CR": 1, "PR": 2, "SD": 3, "NON-CR/NON-PD": 4, "PD": 5, "NE": 6},
            default=None,
            return_dtype=pl.Int64,
        ),
        ADT=pl.col("BESTDT"),
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "PARAM", "RANDDT", "AVALC", "AVAL", "ADT")
    .sort("USUBJID")
)

Path("/app/output").mkdir(exist_ok=True)
adrs.write_csv("/app/output/adrs.csv")
