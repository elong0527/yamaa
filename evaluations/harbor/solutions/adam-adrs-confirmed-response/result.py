# Reference solution for the yamaa benchmark adam-adrs-confirmed-response (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

rs = pl.read_csv("/app/input/rs.csv", infer_schema=False).with_columns(
    pl.col("RSSEQ").cast(pl.Int64),
    pl.col("RSDTC").str.to_date(),
)

# Each assessment is compared with the next one in analysis date order;
# assessments sharing a date are ordered by sequence number, so the
# higher-numbered one is zero days later and never confirms the lower.
ordered = (
    rs.with_columns(
        PARAMCD=pl.lit("CONFRESP"), ADT=pl.col("RSDTC"), AVALC=pl.col("RSSTRESC")
    )
    .sort(["STUDYID", "USUBJID", "ADT", "RSSEQ"])
    .with_columns(
        NEXT_AVALC=pl.col("AVALC").shift(-1).over(["STUDYID", "USUBJID"]),
        NEXT_ADT=pl.col("ADT").shift(-1).over(["STUDYID", "USUBJID"]),
    )
    .with_columns(DAYS=(pl.col("NEXT_ADT") - pl.col("ADT")).dt.total_days())
    .with_columns(
        CONFIRMED=pl.when(pl.col("AVALC") == "PD")
        .then(pl.lit("Y"))
        .when(
            pl.col("AVALC").is_in(["PR", "CR"])
            & pl.col("NEXT_AVALC").is_in(["PR", "CR"])
            & (pl.col("DAYS") >= 28)
        )
        .then(pl.lit("Y"))
        .otherwise(pl.lit("N"))
    )
    .select("STUDYID", "USUBJID", "PARAMCD", "RSSEQ", "ADT", "AVALC", "CONFIRMED")
    .sort(["USUBJID", "ADT", "RSSEQ"])
)

Path("/app/output").mkdir(exist_ok=True)
ordered.write_csv("/app/output/adrs.csv")
