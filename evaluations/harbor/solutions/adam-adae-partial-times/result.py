# Reference solution for the yamaa benchmark adam-adae-partial-times (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

# The parts a collected value is missing, by its length, with the flag that
# names the coarsest part supplied. A time to the second needs nothing.
FIRST = {19: ("", None), 16: (":00", "S"), 13: (":00:00", "M"), 10: ("T00:00:00", "H")}
LAST = {19: ("", None), 16: (":59", "S"), 13: (":59:59", "M"), 10: ("T23:59:59", "H")}


def complete(collected: str | None, fill: dict) -> tuple[str | None, str | None]:
    # A value without its day, or no value at all, gives no analysis time.
    if collected is None or len(collected) not in fill:
        return None, None
    suffix, flag = fill[len(collected)]
    return collected + suffix, flag


rows = []
for record in ae.iter_rows(named=True):
    astdtm, asttmf = complete(record["AESTDTC"], FIRST)
    aendtm, aentmf = complete(record["AEENDTC"], LAST)
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "AESEQ": record["AESEQ"],
            "AETERM": record["AETERM"],
            "AESTDTC": record["AESTDTC"],
            "ASTDTM": astdtm,
            "ASTTMF": asttmf,
            "AEENDTC": record["AEENDTC"],
            "AENDTM": aendtm,
            "AENTMF": aentmf,
        }
    )

adae = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "AESEQ": pl.Int64,
        "AETERM": pl.String,
        "AESTDTC": pl.String,
        "ASTDTM": pl.String,
        "ASTTMF": pl.String,
        "AEENDTC": pl.String,
        "AENDTM": pl.String,
        "AENTMF": pl.String,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
