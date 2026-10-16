# Reference solution for the yamaa benchmark adam-adae-post-dose-window (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import datetime, timedelta
from pathlib import Path

import polars as pl

ae = pl.read_csv("/app/input/ae.csv", infer_schema=False).with_columns(
    pl.col("AESEQ").cast(pl.Int64)
)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False).with_columns(
    pl.col("EXSEQ").cast(pl.Int64)
)


def complete_seconds(value: str | None) -> str | None:
    # Datetimes collected without seconds are completed to the second so
    # onsets and dose starts compare at the moment, to the second.
    if value is None or len(value) != 16:
        return value
    return value + ":00"


def has_time(value: str | None) -> bool:
    return value is not None and len(value) > 10


# The subject's doses, numbered 1, 2, 3 in order of start.
doses: dict[str, list[tuple[datetime, int]]] = {}
ex_rows = ex.with_columns(
    START=pl.col("EXSTDTC").map_elements(
        complete_seconds, return_dtype=pl.String
    )
).sort(["USUBJID", "START", "EXSEQ"])
for record in ex_rows.iter_rows(named=True):
    start = datetime.fromisoformat(record["START"])
    subject_doses = doses.setdefault(record["USUBJID"], [])
    subject_doses.append((start, len(subject_doses) + 1))

adae_rows = []
for record in ae.iter_rows(named=True):
    onset_text = record["AESTDTC"]
    astdt = onset_text[:10] if onset_text is not None else None
    astdtm = complete_seconds(onset_text) if has_time(onset_text) else None
    dose24fl = None
    dosenum = None
    if astdtm is not None:
        onset = datetime.fromisoformat(astdtm)
        # A window opens at the dose start and closes 24 hours later, the
        # closing moment itself outside. When two windows hold the
        # onset, the later dose is counted.
        for start, number in doses.get(record["USUBJID"], []):
            if start <= onset < start + timedelta(hours=24):
                dose24fl = "Y"
                dosenum = number
    adae_rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "AESEQ": record["AESEQ"],
            "AEDECOD": record["AEDECOD"],
            "ASTDT": astdt,
            "ASTDTM": astdtm,
            "DOSE24FL": dose24fl,
            "DOSENUM": dosenum,
        }
    )

adae = pl.DataFrame(
    adae_rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "AESEQ": pl.Int64,
        "AEDECOD": pl.String,
        "ASTDT": pl.String,
        "ASTDTM": pl.String,
        "DOSE24FL": pl.String,
        "DOSENUM": pl.Int64,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adae.write_csv("/app/output/adae.csv")
