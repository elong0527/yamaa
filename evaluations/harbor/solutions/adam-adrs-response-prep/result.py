# Reference solution for the yamaa benchmark adam-adrs-response-prep (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

raw = pl.read_csv("/app/input/adrs_raw.csv", infer_schema=False).with_columns(
    pl.col("ASEQ").cast(pl.Int64),
    pl.col("ADT").str.to_date(),
    pl.col("RANDDY").cast(pl.Int64),
)

# The response category each record can support; stable and
# neither-complete-nor-progressive disease count only on or after day 42,
# and earlier ones, or ones with no day, fall back to not evaluable.
based = raw.with_columns(
    BORCAT=pl.when(pl.col("AVALC").is_in(["CR", "PR", "PD", "NE"]))
    .then(pl.col("AVALC"))
    .when((pl.col("AVALC") == "SD") & (pl.col("RANDDY") >= 42))
    .then(pl.lit("SD"))
    .when(pl.col("AVALC") == "SD")
    .then(pl.lit("NE"))
    .when((pl.col("AVALC") == "NON-CR/NON-PD") & (pl.col("RANDDY") >= 42))
    .then(pl.lit("NON-CR/NON-PD"))
    .when(pl.col("AVALC") == "NON-CR/NON-PD")
    .then(pl.lit("NE")),
).with_columns(
    BORPRI=pl.col("BORCAT").replace_strict(
        {"CR": 1, "PR": 2, "SD": 3, "NON-CR/NON-PD": 4, "PD": 5, "NE": 6},
        default=None,
        return_dtype=pl.Int64,
    )
)

# Usable records numbered from 1 in priority, date, then sequence order;
# records supporting no category are passed over without a gap.
usable = (
    based.filter(pl.col("BORCAT").is_not_null())
    .sort(["STUDYID", "USUBJID", "BORPRI", "ADT", "ASEQ"])
    .with_columns(
        BORSEQ=pl.col("ASEQ").cum_count().over(["STUDYID", "USUBJID"]).cast(pl.Int64)
    )
    .select("STUDYID", "USUBJID", "ASEQ", "BORSEQ")
)

adrs = (
    based.join(usable, on=["STUDYID", "USUBJID", "ASEQ"], how="left")
    .select(
        "STUDYID", "USUBJID", "ASEQ", "ADT", "RANDDY",
        "AVALC", "BORCAT", "BORPRI", "BORSEQ",
    )
    .sort(["STUDYID", "USUBJID", "ASEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
adrs.write_csv("/app/output/adrs.csv")
