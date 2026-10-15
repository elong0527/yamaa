# Reference solution for the yamaa benchmark adam-adpp-period-treatment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

pp = pl.read_csv("/app/input/pp.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)

# The subject's dose that starts exactly at a reference date and time.
dose = {
    (record["USUBJID"], record["EXSTDTC"]): record for record in ex.iter_rows(named=True)
}

rows = []
for record in pp.iter_rows(named=True):
    matched = dose.get((record["USUBJID"], record["PPRFTDTC"]))
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "PARCAT1": record["PPCAT"],
            "PARAMCD": record["PPTESTCD"],
            "PARAM": record["PPTEST"],
            "PPRFTDTC": record["PPRFTDTC"],
            # TREATMENT 1 is period 1, TREATMENT 2 period 2.
            "APERIOD": int(matched["EPOCH"].split()[-1]) if matched else None,
            "TRTA": matched["EXTRT"] if matched else None,
            "DOSEA": float(matched["EXDOSE"]) if matched else None,
            "DOSEU": matched["EXDOSU"] if matched else None,
            "AVAL": float(record["PPSTRESN"]),
            "AVALU": record["PPSTRESU"],
        }
    )

adpp = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "PARCAT1": pl.String,
        "PARAMCD": pl.String,
        "PARAM": pl.String,
        "PPRFTDTC": pl.String,
        "APERIOD": pl.Int64,
        "TRTA": pl.String,
        "DOSEA": pl.Float64,
        "DOSEU": pl.String,
        "AVAL": pl.Float64,
        "AVALU": pl.String,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adpp.write_csv("/app/output/adpp.csv")
