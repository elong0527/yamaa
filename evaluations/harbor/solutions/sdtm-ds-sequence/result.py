# Reference solution for the yamaa benchmark sdtm-ds-sequence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def complete_date(raw: str | None) -> str | None:
    if raw is None:
        return None
    s = raw.strip()
    if s == "":
        return None
    parts = s.split("-")
    try:
        if len(parts) == 3:
            y, m, d = int(parts[0]), int(parts[1]), int(parts[2])
            return f"{y:04d}-{m:02d}-{d:02d}" if len(parts[0]) == 4 else None
        if len(parts) == 2 and len(parts[0]) == 4:
            y, m = int(parts[0]), int(parts[1])
            if 1 <= m <= 12:
                return f"{y:04d}-{m:02d}-15"
            return None
        if len(parts) == 1 and len(s) == 4:
            y = int(s)
            return f"{y:04d}-06-15"
        return None
    except ValueError:
        return None


ds_raw = pl.read_csv("/app/input/ds_raw.csv", infer_schema=False)

records: list[dict] = []
for row in ds_raw.to_dicts():
    study = row["STUDY"]
    pat = row["PATNUM"]
    decod = row["DSDECOD"]
    raw = row["DSDTCOL"]
    completed = complete_date(raw)
    # Validate the completed date parses.
    check = pl.DataFrame({"d": [completed]}).with_columns(
        pl.col("d").str.to_date(strict=False).alias("dt")
    )["dt"][0]
    if check is None:
        completed = None
    dscat = "PROTOCOL MILESTONE" if decod == "RANDOMIZED" else "DISPOSITION EVENT"
    records.append(
        {
            "STUDYID": study,
            "USUBJID": pat,
            "DSDECOD": decod,
            "DSCAT": dscat,
            "DSDTC": completed,
        }
    )

ds = pl.DataFrame(
    records,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "DSDECOD": pl.String,
        "DSCAT": pl.String,
        "DSDTC": pl.String,
    },
).with_columns(DOMAIN=pl.lit("DS"), DSDTC_DT=pl.col("DSDTC").str.to_date(strict=False))

ds = (
    ds.sort(["USUBJID", "DSDTC_DT", "DSDECOD"], nulls_last=True)
    .with_columns(DSSEQ=pl.col("USUBJID").cum_count().over("USUBJID"))
    .with_columns(DSSEQ=pl.col("DSSEQ").cast(pl.Int64))
    .select("DOMAIN", "STUDYID", "USUBJID", "DSSEQ", "DSDECOD", "DSCAT", "DSDTC")
)

Path("/app/output").mkdir(exist_ok=True)
ds.write_csv("/app/output/ds.csv")
