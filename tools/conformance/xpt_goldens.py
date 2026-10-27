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


# Minimum clinical guide witnesses; the source tables are authored in each
# benchmark expected CSV. Guide versions do not change XPT v5 bytes.
GOLDENS.extend(
    [
        (
            "sdtm-dm-sdtmig33",
            "dm.xpt",
            "DM",
            "Demographics",
            DM_VARIABLES,
            DM_ROWS,
        ),
        (
            "adam-adsl-adamig11",
            "adsl.xpt",
            "ADSL",
            "Subject-Level Analysis Dataset",
            ADSL_VARIABLES,
            ADSL_ROWS,
        ),
        (
            "adam-adsl-adamig12",
            "adsl.xpt",
            "ADSL",
            "Subject-Level Analysis Dataset",
            ADSL_VARIABLES,
            ADSL_ROWS,
        ),
        (
            "sdtm-sv-sdtmig33",
            "sv.xpt",
            "SV",
            "Subject Visits",
            [
                Variable(
                    name="DOMAIN",
                    label="Domain Abbreviation",
                    character=True,
                    width=2,
                    format=None,
                ),
                Variable(
                    name="STUDYID",
                    label="Study Identifier",
                    character=True,
                    width=4,
                    format=None,
                ),
                Variable(
                    name="USUBJID",
                    label="Unique Subject Identifier",
                    character=True,
                    width=7,
                    format=None,
                ),
                Variable(
                    name="VISITNUM",
                    label="Visit Number",
                    character=False,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="VISIT",
                    label="Visit Name",
                    character=True,
                    width=11,
                    format=None,
                ),
                Variable(
                    name="SVSTDTC",
                    label="Start Date/Time of Visit",
                    character=True,
                    width=10,
                    format=None,
                ),
                Variable(
                    name="SVENDTC",
                    label="End Date/Time of Visit",
                    character=True,
                    width=10,
                    format=None,
                ),
                Variable(
                    name="SVUPDES",
                    label="Description of Unplanned Visit",
                    character=True,
                    width=10,
                    format=None,
                ),
            ],
            [
                [
                    "SV",
                    "CATH",
                    "CATH-01",
                    1.0,
                    "SCREENING",
                    "2025-01-03",
                    "2025-01-04",
                    None,
                ],
                [
                    "SV",
                    "CATH",
                    "CATH-01",
                    2.0,
                    "DAY 1",
                    "2025-01-08",
                    "2025-01-08",
                    None,
                ],
                [
                    "SV",
                    "CATH",
                    "CATH-01",
                    2.5,
                    "UNSCHEDULED",
                    "2025-01-10",
                    "2025-01-10",
                    "REPEAT LAB",
                ],
            ],
        ),
        (
            "sdtm-mo-sdtmig33",
            "mo.xpt",
            "MO",
            "Morphology",
            [
                Variable(
                    name="DOMAIN",
                    label="Domain Abbreviation",
                    character=True,
                    width=2,
                    format=None,
                ),
                Variable(
                    name="STUDYID",
                    label="Study Identifier",
                    character=True,
                    width=4,
                    format=None,
                ),
                Variable(
                    name="USUBJID",
                    label="Unique Subject Identifier",
                    character=True,
                    width=7,
                    format=None,
                ),
                Variable(
                    name="MOSEQ",
                    label="Sequence Number",
                    character=False,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="MOTESTCD",
                    label="Morphology Test Short Name",
                    character=True,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="MOTEST",
                    label="Morphology Test Name",
                    character=True,
                    width=9,
                    format=None,
                ),
                Variable(
                    name="MOORRES",
                    label="Result or Finding in Original Units",
                    character=True,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="MOSTRESC",
                    label="Character Result/Finding in Std Format",
                    character=True,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="MODTC",
                    label="Date/Time of Collection",
                    character=True,
                    width=10,
                    format=None,
                ),
            ],
            [
                [
                    "MO",
                    "CATH",
                    "CATH-01",
                    1,
                    "CELLTYPE",
                    "Cell Type",
                    "normal",
                    "NORMAL",
                    "2025-01-08",
                ],
                [
                    "MO",
                    "CATH",
                    "CATH-02",
                    1,
                    "CELLTYPE",
                    "Cell Type",
                    "abnormal",
                    "ABNORMAL",
                    "2025-01-09",
                ],
            ],
        ),
        (
            "adam-adlb-adamig11",
            "adlb.xpt",
            "ADLB",
            "Laboratory Analysis Dataset",
            [
                Variable(
                    name="STUDYID",
                    label="Study Identifier",
                    character=True,
                    width=4,
                    format=None,
                ),
                Variable(
                    name="USUBJID",
                    label="Unique Subject Identifier",
                    character=True,
                    width=7,
                    format=None,
                ),
                Variable(
                    name="PARAMCD",
                    label="Parameter Code",
                    character=True,
                    width=3,
                    format=None,
                ),
                Variable(
                    name="PARAM",
                    label="Parameter",
                    character=True,
                    width=24,
                    format=None,
                ),
                Variable(
                    name="ADT",
                    label="Analysis Date",
                    character=False,
                    width=8,
                    format=Format(name="DATE", width=9, decimals=0),
                ),
                Variable(
                    name="TRT01A",
                    label="Actual Treatment for Period 01",
                    character=True,
                    width=7,
                    format=None,
                ),
                Variable(
                    name="ADY",
                    label="Analysis Relative Day",
                    character=False,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="AVAL",
                    label="Analysis Value",
                    character=False,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="AVALU",
                    label="Analysis Value Unit",
                    character=True,
                    width=3,
                    format=None,
                ),
                Variable(
                    name="ABLFL",
                    label="Baseline Record Flag",
                    character=True,
                    width=1,
                    format=None,
                ),
                Variable(
                    name="BASE",
                    label="Baseline Value",
                    character=False,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="CHG",
                    label="Change from Baseline",
                    character=False,
                    width=8,
                    format=None,
                ),
            ],
            [
                [
                    "CATH",
                    "CATH-01",
                    "ALT",
                    "Alanine Aminotransferase",
                    23748,
                    "DRUG",
                    -1,
                    10.0,
                    "U/L",
                    "Y",
                    10.0,
                    0.0,
                ],
                [
                    "CATH",
                    "CATH-01",
                    "ALT",
                    "Alanine Aminotransferase",
                    23750,
                    "DRUG",
                    2,
                    15.0,
                    "U/L",
                    None,
                    10.0,
                    5.0,
                ],
                [
                    "CATH",
                    "CATH-02",
                    "ALT",
                    "Alanine Aminotransferase",
                    23750,
                    "PLACEBO",
                    2,
                    20.0,
                    "U/L",
                    None,
                    None,
                    None,
                ],
            ],
        ),
        (
            "adam-adae-adamig11",
            "adae.xpt",
            "ADAE",
            "Adverse Events Analysis Dataset",
            [
                Variable(
                    name="STUDYID",
                    label="Study Identifier",
                    character=True,
                    width=4,
                    format=None,
                ),
                Variable(
                    name="USUBJID",
                    label="Unique Subject Identifier",
                    character=True,
                    width=16,
                    format=None,
                ),
                Variable(
                    name="AESEQ",
                    label="Sequence Number",
                    character=False,
                    width=8,
                    format=None,
                ),
                Variable(
                    name="AETERM",
                    label="Reported Term for the Adverse Event",
                    character=True,
                    width=9,
                    format=None,
                ),
                Variable(
                    name="ASTDTM",
                    label="Analysis Start Date/Time",
                    character=False,
                    width=8,
                    format=Format(name="DATETIME", width=20, decimals=0),
                ),
                Variable(
                    name="ASTDT",
                    label="Analysis Start Date",
                    character=False,
                    width=8,
                    format=Format(name="DATE", width=9, decimals=0),
                ),
                Variable(
                    name="ASTTM",
                    label="Analysis Start Time",
                    character=False,
                    width=8,
                    format=Format(name="TIME", width=8, decimals=0),
                ),
                Variable(
                    name="ASTTMF",
                    label="Analysis Start Time Imputation Flag",
                    character=True,
                    width=1,
                    format=None,
                ),
            ],
            [
                [
                    "CATH",
                    "CATH-UCSD-0001",
                    1,
                    "HEADACHE",
                    2056696215,
                    23804,
                    30615,
                    None,
                ],
                ["CATH", "CATH-UCSD-0001", 2, "NAUSEA", 2056889100, 23806, 50700, "S"],
                ["CATH", "CATH-UCSD-0002", 1, "RASH", 2057184000, 23810, 0, "H"],
                ["CATH", "CATH-UCSD-0002", 2, "DIZZINESS", None, None, None, None],
                [
                    "CATH",
                    "CATH-MCGILL-0003",
                    1,
                    "FATIGUE",
                    2057432400,
                    23812,
                    75600,
                    "M",
                ],
            ],
        ),
    ]
)


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
