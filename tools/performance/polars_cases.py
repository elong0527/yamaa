"""The bds and copy cases in polars (lazy), written the way a polars user
would write them.

Usage: python polars_cases.py CASE DIRECTORY. `bds` repeats
cases/bds/spec.yaml's derivation: it reads DIRECTORY/input/lb.parquet and
DIRECTORY/input/adsl.parquet and writes DIRECTORY/adlb.parquet. `copy` reads
DIRECTORY/input/lb.parquet and writes it unchanged to DIRECTORY/lb.parquet,
as cases/copy/spec.yaml does. The thread count is polars' own, set with
POLARS_MAX_THREADS. This file imports nothing from the repository, so its
start-up is polars' alone.
"""

import sys
from pathlib import Path

import polars as pl

GROUP = ["STUDYID", "USUBJID", "PARAMCD"]
COLUMNS = [
    "STUDYID",
    "USUBJID",
    "PARAMCD",
    "PARAM",
    "ADT",
    "TRTSDT",
    "TRT01A",
    "AVAL",
    "AVALU",
    "ABLFL",
    "BASE",
    "CHG",
    "PCHG",
    "ASEQ",
]


def derive(lb, adsl):
    """ADLB from the LB and ADSL frames, lazy, in the spec's key order."""
    numeric = pl.col("LBSTRESN").is_not_null()
    alt = lb.filter((pl.col("LBTESTCD") == "ALT") & numeric)
    ast = lb.filter((pl.col("LBTESTCD") == "AST") & numeric)
    rows = pl.concat(
        [
            alt.with_columns(
                PARAMCD=pl.lit("ALT"),
                PARAM=pl.lit("Alanine Aminotransferase"),
                AVAL=pl.col("LBSTRESN"),
                AVALU=pl.col("LBSTRESU"),
            ),
            alt.with_columns(
                PARAMCD=pl.lit("ALTSI"),
                PARAM=pl.lit("Alanine Aminotransferase (SI)"),
                AVAL=pl.col("LBSTRESN") * 0.0167,
                AVALU=pl.lit("ukat/L"),
            ),
            ast.with_columns(
                PARAMCD=pl.lit("AST"),
                PARAM=pl.lit("Aspartate Aminotransferase"),
                AVAL=pl.col("LBSTRESN"),
                AVALU=pl.col("LBSTRESU"),
            ),
        ]
    )
    rows = rows.join(
        adsl.select("STUDYID", "USUBJID", "TRTSDT", "TRT01A"),
        on=["STUDYID", "USUBJID"],
        how="left",
    ).with_columns(ADT=pl.col("LBDTC"))
    # Baseline: the latest record on or before treatment start with a value.
    eligible = (pl.col("ADT") <= pl.col("TRTSDT")) & pl.col("AVAL").is_not_null()
    latest = pl.when(eligible).then(pl.col("ADT")).max().over(GROUP)
    rows = rows.with_columns(
        ABLFL=pl.when(eligible & (pl.col("ADT") == latest)).then(pl.lit("Y"))
    )
    rows = rows.with_columns(
        BASE=pl.col("AVAL").filter(pl.col("ABLFL") == "Y").first().over(GROUP)
    )
    rows = rows.with_columns(CHG=pl.col("AVAL") - pl.col("BASE")).with_columns(
        PCHG=100 * pl.col("CHG") / pl.when(pl.col("BASE") != 0).then(pl.col("BASE"))
    )
    rows = rows.sort(["STUDYID", "USUBJID", "PARAMCD", "ADT"]).with_columns(
        ASEQ=pl.int_range(1, pl.len() + 1, dtype=pl.Int64).over(["STUDYID", "USUBJID"])
    )
    return rows.select(COLUMNS)


def main():
    case, directory = sys.argv[1], Path(sys.argv[2])
    lb = pl.scan_parquet(directory / "input" / "lb.parquet")
    if case == "bds":
        adsl = pl.scan_parquet(directory / "input" / "adsl.parquet")
        derive(lb, adsl).sink_parquet(directory / "adlb.parquet")
    elif case == "copy":
        lb.sink_parquet(directory / "lb.parquet")
    else:
        sys.exit(f"unknown case {case!r}: bds or copy")


if __name__ == "__main__":
    main()
