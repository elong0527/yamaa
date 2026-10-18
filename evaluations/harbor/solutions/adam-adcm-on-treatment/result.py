# Reference solution for the yamaa benchmark adam-adcm-on-treatment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADCM (one record per concomitant medication) from ADSL and CM.

ONTRTFL is "Y" when the medication's analysis dates overlap the subject's
treatment period (TRTSDT through TRTEDT, inclusive). A medication with a
missing start or end date is treated as overlapping unless its known dates
rule overlap out. A missing TRTEDT leaves the period open-ended, and a
subject with no TRTSDT is never flagged.
"""

import polars as pl

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "CMSEQ",
    "CMTRT",
    "ASTDT",
    "AENDT",
    "TRTSDT",
    "TRTEDT",
    "ONTRTFL",
]


def read_csv(name):
    frame = pl.read_csv(
        f"{INPUT_DIR}/{name}",
        schema_overrides={
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "CMSEQ": pl.String,
            "CMTRT": pl.String,
            "CMSTDTC": pl.String,
            "CMENDTC": pl.String,
            "TRTSDT": pl.String,
            "TRTEDT": pl.String,
        },
    )
    return frame.fill_null("")


def main():
    adsl = read_csv("adsl.csv")
    cm = read_csv("cm.csv")

    trt = {
        row["USUBJID"]: (row["TRTSDT"], row["TRTEDT"])
        for row in adsl.iter_rows(named=True)
    }

    records = []
    for row in cm.iter_rows(named=True):
        astdt, aendt = row["CMSTDTC"], row["CMENDTC"]
        trtsdt, trtedt = trt.get(row["USUBJID"], ("", ""))

        ontrtfl = ""
        if trtsdt:
            ends_before_start = bool(aendt) and aendt < trtsdt
            starts_after_end = bool(astdt) and bool(trtedt) and astdt > trtedt
            if not ends_before_start and not starts_after_end:
                ontrtfl = "Y"

        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "CMSEQ": row["CMSEQ"],
                "CMTRT": row["CMTRT"],
                "ASTDT": astdt,
                "AENDT": aendt,
                "TRTSDT": trtsdt,
                "TRTEDT": trtedt,
                "ONTRTFL": ontrtfl,
            }
        )

    adcm = pl.DataFrame(records, schema={name: pl.String for name in COLUMNS})
    # Write empty values as bare empty fields, not quoted "" strings.
    adcm = adcm.select(
        pl.when(pl.col(name) == "")
        .then(pl.lit(None, dtype=pl.String))
        .otherwise(pl.col(name))
        .alias(name)
        for name in COLUMNS
    )
    adcm.write_csv(f"{OUTPUT_DIR}/adcm.csv")


if __name__ == "__main__":
    main()
