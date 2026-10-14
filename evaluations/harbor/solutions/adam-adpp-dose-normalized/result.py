# Reference solution for the yamaa benchmark adam-adpp-dose-normalized (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

COLUMNS = ["STUDYID", "USUBJID", "VISIT", "PARAMCD", "PARAM", "AVAL", "AVALU"]

# The three exposure parameters that get a dose-normalized record.
NORMALIZED = pl.DataFrame(
    {
        "PPTESTCD": ["CMAX", "AUCLST", "AUCIFO"],
        "NEWCD": ["CMAXD", "AUCLSTD", "AUCIFOD"],
        "NEWPARAM": [
            "Max Conc Norm by Dose",
            "AUC to Last Nonzero Conc Norm by Dose",
            "AUC Infinity Obs Norm by Dose",
        ],
    }
)
MG_PER_UNIT = pl.DataFrame({"EXDOSU": ["mg", "ug"], "MGDIV": [1.0, 1000.0]})

pp = pl.read_csv("/app/input/pp.csv", infer_schema=False).with_columns(
    pl.col("PPSTRESN").cast(pl.Float64)
)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False).with_columns(
    pl.col("EXDOSE").cast(pl.Float64)
)

# Every PP record as collected.
kept = pp.select(
    "STUDYID",
    "USUBJID",
    "VISIT",
    PARAMCD=pl.col("PPTESTCD"),
    PARAM=pl.col("PPTEST"),
    AVAL=pl.col("PPSTRESN"),
    AVALU=pl.col("PPSTRESU"),
)

# The one administration that starts at each profile's reference time, its
# dose in mg (a dose in ug is a thousandth of a mg).
dose = ex.join(MG_PER_UNIT, on="EXDOSU", how="left").select(
    "STUDYID",
    "USUBJID",
    PPRFTDTC=pl.col("EXSTDTC"),
    DOSEMG=pl.col("EXDOSE") / pl.col("MGDIV"),
)

added = (
    pp.join(NORMALIZED, on="PPTESTCD", how="inner")
    .filter(pl.col("PPSTRESN").is_not_null())
    .join(dose, on=["STUDYID", "USUBJID", "PPRFTDTC"], how="inner")
    .filter(pl.col("DOSEMG").is_not_null() & (pl.col("DOSEMG") != 0))
    .select(
        "STUDYID",
        "USUBJID",
        "VISIT",
        PARAMCD=pl.col("NEWCD"),
        PARAM=pl.col("NEWPARAM"),
        AVAL=pl.col("PPSTRESN") / pl.col("DOSEMG"),
        AVALU=pl.col("PPSTRESU") + "/mg",
    )
)

adpp = pl.concat([kept, added]).select(COLUMNS)

Path("/app/output").mkdir(exist_ok=True)
adpp.write_csv("/app/output/adpp.csv")
