"""Write the transport benchmarks' ``.xpt`` goldens from hand-authored tables.

Each table below is authored literally from the benchmark's independently
written expected dataset: names, labels, character widths (the longest value
of each column, in bytes), display formats and cell values. SAS dates and
datetimes are counted here from the civil values with the standard library.
``python -m tools.conformance.xpt_goldens`` rewrites the goldens; the tests
re-encode every table and compare it with the committed bytes.
"""

import argparse
import datetime

from tools.conformance.xpt_oracle import (
    Format,
    Variable,
    encode,
    sas_date,
    sas_datetime,
)
from tools.paths import ROOT

CREATED = datetime.datetime(2026, 9, 11, 0, 0, 0)


def character(name, label, width):
    return Variable(name, label, character=True, width=width)


def numeric(name, label, form=None):
    return Variable(name, label, character=False, format=form)


DM_VARIABLES = [
    character("DOMAIN", "Domain Abbreviation", 2),
    character("STUDYID", "Study Identifier", 4),
    character("USUBJID", "Unique Subject Identifier", 16),
    character("SUBJID", "Subject Identifier for the Study", 4),
    character("RFSTDTC", "Subject Reference Start Date/Time", 10),
    character("SITEID", "Study Site Identifier", 6),
    numeric("AGE", "Age"),
    character("AGEU", "Age Units", 5),
    character("SEX", "Sex", 1),
    character("RACE", "Race", 25),
    character("COUNTRY", "Country", 3),
]
# sdtm-dm-xpt/expected/dm.csv; an SDTM date is its canonical text.
DM_ROWS = [
    [
        "DM",
        "CATH",
        "CATH-UCSD-0001",
        "0001",
        "2025-01-08",
        "UCSD",
        34,
        "YEARS",
        "F",
        "WHITE",
        "USA",
    ],
    [
        "DM",
        "CATH",
        "CATH-UCSD-0002",
        "0002",
        "2025-01-15",
        "UCSD",
        67,
        "YEARS",
        "M",
        "BLACK OR AFRICAN AMERICAN",
        "USA",
    ],
    [
        "DM",
        "CATH",
        "CATH-MCGILL-0003",
        "0003",
        None,
        "MCGILL",
        52,
        "YEARS",
        "F",
        "ASIAN",
        "CAN",
    ],
]

ADSL_VARIABLES = [
    character("STUDYID", "Study Identifier", 4),
    character("USUBJID", "Unique Subject Identifier", 16),
    character("TRT01A", "Actual Treatment for Period 01", 11),
    numeric("AGE", "Age", Format("", 3)),
    numeric("HEIGHTBL", "Baseline Height (cm)", Format("", 5, 1)),
    numeric("TRTSDT", "Date of First Exposure to Treatment", Format("DATE", 9)),
    numeric(
        "TRTSDTM", "Datetime of First Exposure to Treatment", Format("DATETIME", 20)
    ),
]
# adam-adsl-xpt/expected/adsl.csv; an ADaM date counts days and a datetime
# seconds from 1960-01-01.
ADSL_ROWS = [
    [
        "CATH",
        "CATH-UCSD-0001",
        "VITAMIN D3",
        34,
        172.5,
        sas_date(datetime.date(2025, 1, 8)),
        sas_datetime(datetime.datetime(2025, 1, 8, 8, 30, 0)),
    ],
    [
        "CATH",
        "CATH-UCSD-0002",
        "PLACEBO",
        67,
        None,
        sas_date(datetime.date(2025, 1, 15)),
        sas_datetime(datetime.datetime(2025, 1, 15, 9, 5, 30)),
    ],
    ["CATH", "CATH-MCGILL-0003", "NOT TREATED", 52, 165.2, None, None],
]

# benchmark, file, member name, member label, variables, rows
GOLDENS = [
    ("sdtm-dm-xpt", "dm.xpt", "DM", "Demographics", DM_VARIABLES, DM_ROWS),
    (
        "adam-adsl-xpt",
        "adsl.xpt",
        "ADSL",
        "Subject-Level Analysis Dataset",
        ADSL_VARIABLES,
        ADSL_ROWS,
    ),
]


def golden(benchmark, name, label, variables, rows):
    """The bytes of one golden, after checking each authored width."""
    for index, variable in enumerate(variables):
        if variable.character:
            longest = max(len((row[index] or "").encode("ascii")) for row in rows)
            if variable.width != max(longest, 1):
                raise ValueError(
                    f"{benchmark}: {variable.name} width is not its longest value"
                )
    return encode(name, label, CREATED, variables, rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    for benchmark, file, name, label, variables, rows in GOLDENS:
        target = ROOT / "benchmarks" / benchmark / "expected" / file
        target.write_bytes(golden(benchmark, name, label, variables, rows))
        print(f"wrote {target.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
