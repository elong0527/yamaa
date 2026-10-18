# Reference solution for the yamaa benchmark adam-adae-analysis-time (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)

# What a collected start is missing, by its length, with the flag that names
# the coarsest part supplied. A time to the second needs nothing.
FIRST = {19: ("", None), 16: (":00", "S"), 13: (":00:00", "M"), 10: ("T00:00:00", "H")}

rows = []
for record in ae.iter_rows(named=True):
    collected = record["AESTDTC"]
    astdtm = astdt = asttm = asttmf = None
    # A start without its day, or no start at all, gives no analysis time.
    if collected is not None and len(collected) in FIRST:
        suffix, asttmf = FIRST[len(collected)]
        astdtm = collected + suffix
        astdt = astdtm[:10]
        hours, minutes, seconds = (int(part) for part in astdtm[11:].split(":"))
        asttm = hours * 3600 + minutes * 60 + seconds
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "AESEQ": record["AESEQ"],
            "AETERM": record["AETERM"],
            "ASTDTM": astdtm,
            "ASTDT": astdt,
            "ASTTM": asttm,
            "ASTTMF": asttmf,
        }
    )

adae = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "AESEQ": pl.Int64,
        "AETERM": pl.String,
        "ASTDTM": pl.String,
        "ASTDT": pl.String,
        "ASTTM": pl.Int64,
        "ASTTMF": pl.String,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
