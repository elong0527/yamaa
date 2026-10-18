# Reference solution for the yamaa benchmark adam-adcm-atc-classes (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADCM with ATC class levels 1-4 per medication record.

Reads CM, FACM and ATC_DICT from /app/input and writes adcm.csv to
/app/output with one record per CM medication record.
"""

import polars as pl

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

LEVELS = ["ATC1", "ATC2", "ATC3", "ATC4"]
OUT_COLUMNS = [
    "STUDYID",
    "USUBJID",
    "CMSEQ",
    "CMTRT",
    "CMDECOD",
    "ATC1",
    "ATC2",
    "ATC3",
    "ATC4",
    "ATC1CD",
    "ATC2CD",
    "ATC3CD",
    "ATC4CD",
]


def read_strings(name: str) -> pl.DataFrame:
    """Read a CSV with every column as a string, preserving blanks."""
    return pl.read_csv(f"{INPUT_DIR}/{name}", infer_schema_length=0)


def main() -> None:
    cm = read_strings("cm.csv")
    facm = read_strings("facm.csv")
    atc_dict = read_strings("atc_dict.csv")

    # ATC code -> class name lookup.
    code_to_name = dict(
        zip(
            atc_dict["ATCCODE"].to_list(),
            atc_dict["ATCNAME"].to_list(),
        )
    )

    # (STUDYID, USUBJID, CMSEQ, FATESTCD) -> FAORRES code, for ATC1-ATC4
    # FACM rows only. FACM rows for other tests are ignored; rows that
    # match no medication record simply never get looked up.
    facm_codes = {}
    for row in facm.iter_rows(named=True):
        if row["FATESTCD"] in LEVELS:
            key = (row["STUDYID"], row["USUBJID"], row["CMSEQ"], row["FATESTCD"])
            facm_codes[key] = row["FAORRES"]

    records = []
    for row in cm.iter_rows(named=True):
        codes = {}
        for level in LEVELS:
            key = (row["STUDYID"], row["USUBJID"], row["CMSEQ"], level)
            codes[level] = facm_codes.get(key) or None
        record = {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "CMSEQ": row["CMSEQ"],
            "CMTRT": row["CMTRT"],
            "CMDECOD": row["CMDECOD"] or None,
        }
        for level in LEVELS:
            code = codes[level]
            record[level] = code_to_name.get(code) if code else None
        for level in LEVELS:
            record[f"{level}CD"] = codes[level]
        records.append(record)

    adcm = pl.DataFrame(
        records, schema={col: pl.Utf8 for col in OUT_COLUMNS}
    )
    adcm.write_csv(f"{OUTPUT_DIR}/adcm.csv")


if __name__ == "__main__":
    main()
