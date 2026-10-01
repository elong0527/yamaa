# Reference solution for the yamaa benchmark adam-adsl-randomization (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_parquet("/app/input/dm.parquet")

# Study day from the reference start: the reference day itself is day 1,
# later dates count forward inclusively, earlier dates count backward with
# no day 0, so the day before is day -1.
diff = (pl.col("RANDDT") - pl.col("RFSTDTC")).dt.total_days()
adsl = (
    dm.with_columns(
        RANDDY=pl.when(diff.is_null())
        .then(None)
        .when(diff >= 0)
        .then(diff + 1)
        .otherwise(diff),
        # The collected moment as ISO 8601 text with a T, at whole-second
        # precision, written from what was collected even when no date was.
        RANDDTC=pl.col("RANDDTTM").dt.strftime("%Y-%m-%dT%H:%M:%S"),
    )
    .select("STUDYID", "USUBJID", "RANDDT", "RANDDY", "RANDDTC")
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adsl.write_csv("/app/output/adsl.csv", null_value="")
