# Reference solution for the yamaa benchmark adam-adpc-relative-times (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import datetime
from pathlib import Path

import polars as pl

pc = pl.read_csv("/app/input/pc.csv", infer_schema=False).with_columns(
    pl.col("PCSEQ").cast(pl.Int64)
)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)

DRUG = {"DRGX": "DRUG X", "MET1": "DRUG X", "DRGY": "DRUG Y"}


def moment(text: str | None) -> datetime | None:
    # A date without a time is not a moment; a time without seconds counts
    # as second zero.
    if text is None or len(text) <= 10:
        return None
    return datetime.fromisoformat(text if len(text) == 19 else text + ":00")


# Each subject's dose starts per drug, in order.
doses: dict[tuple[str, str], list[datetime]] = {}
for record in ex.iter_rows(named=True):
    start = moment(record["EXSTDTC"])
    if start is not None:
        doses.setdefault((record["USUBJID"], record["EXTRT"]), []).append(start)
for starts in doses.values():
    starts.sort()


def text(value: datetime | None) -> str | None:
    return None if value is None else value.isoformat()


def hours(later: datetime | None, earlier: datetime | None) -> float | None:
    if later is None or earlier is None:
        return None
    return (later - earlier).total_seconds() / 3600


rows = []
for record in pc.iter_rows(named=True):
    adtm = moment(record["PCDTC"])
    starts = doses.get((record["USUBJID"], DRUG.get(record["PCTESTCD"])), [])
    first = starts[0] if starts else None
    reference = None
    if adtm is not None and first is not None:
        # The latest dose at or before the sample; the first dose for a
        # sample taken before every dose.
        reference = first
        for start in starts:
            if start <= adtm:
                reference = start
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": record["USUBJID"],
            "PCSEQ": record["PCSEQ"],
            "PARAMCD": record["PCTESTCD"],
            "ATPT": record["PCTPT"],
            "ADTM": text(adtm),
            "FANLDTM": text(first),
            "PCRFTDTM": text(reference),
            "AFRLT": hours(adtm, first),
            "ARRLT": hours(adtm, reference),
        }
    )

adpc = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "PCSEQ": pl.Int64,
        "PARAMCD": pl.String,
        "ATPT": pl.String,
        "ADTM": pl.String,
        "FANLDTM": pl.String,
        "PCRFTDTM": pl.String,
        "AFRLT": pl.Float64,
        "ARRLT": pl.Float64,
    },
)

Path("/app/output").mkdir(exist_ok=True)
adpc.write_csv("/app/output/adpc.csv")
