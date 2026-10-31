"""Deterministic inputs for the performance cases, written as Parquet with
polars: an ADSL and LB pair at a subject count, and a one-row-per-subject
vitals table for the project-function case.

Every value is integer arithmetic on the row's position, so a size always
gives the same tables, on any machine and any polars release, with no random
number generator. LB has 6 lab tests x 12 visits per subject, sorted by
subject, test code and date; two visits fall before treatment start (days -7
and -1), and about 4% of results are missing, never on day -1, so every
subject and test has a baseline record.
"""

import datetime

import polars as pl

STUDY = "PERF-01"
TESTS = (
    # (LBTESTCD, LBTEST, LBSTRESU, lowest result), sorted by test code.
    ("ALP", "Alkaline Phosphatase", "U/L", 40.0),
    ("ALT", "Alanine Aminotransferase", "U/L", 10.0),
    ("AST", "Aspartate Aminotransferase", "U/L", 12.0),
    ("BILI", "Bilirubin", "umol/L", 3.0),
    ("CREAT", "Creatinine", "umol/L", 50.0),
    ("GLUC", "Glucose", "mmol/L", 3.5),
)
DAYS = (-7, -1, 8, 15, 22, 29, 43, 57, 85, 113, 141, 169)
BASELINE_VISIT = DAYS.index(-1)
ROWS_PER_SUBJECT = len(TESTS) * len(DAYS)
ARMS = ("Placebo", "Study Drug 50 mg", "Study Drug 100 mg")
FIRST_TREATMENT = datetime.date(2025, 1, 6)


def _subject_id(number):
    """`PERF-01-000001` for subject number 1, from an integer expression."""
    return pl.concat_str(pl.lit(f"{STUDY}-"), number.cast(pl.String).str.zfill(6))


def _treatment_start(number):
    """A subject's TRTSDT: one of 28 consecutive days, by subject number."""
    return pl.lit(FIRST_TREATMENT) + pl.duration(days=number % 28)


def _mix(*terms):
    """A well-spread integer in [0, 1,000,003) of the row's coordinates: a
    linear combination, squared modulo a prime."""
    expression = pl.lit(0, pl.Int64)
    for term, factor in terms:
        expression = expression + (term + 1) * factor
    expression = expression % 1_000_003
    return expression * expression % 1_000_003


def adsl(subjects):
    """One record per subject: STUDYID, USUBJID, TRTSDT, TRT01A."""
    number = pl.col("n")
    return pl.DataFrame({"n": pl.int_range(1, subjects + 1, eager=True)}).select(
        pl.lit(STUDY).alias("STUDYID"),
        _subject_id(number).alias("USUBJID"),
        _treatment_start(number).alias("TRTSDT"),
        (number % len(ARMS))
        .replace_strict(dict(enumerate(ARMS)), return_dtype=pl.String)
        .alias("TRT01A"),
    )


def lb(subjects):
    """`subjects` x 72 records with the 7 columns of an SDTM LB extract."""
    row = pl.col("r")
    number = row // ROWS_PER_SUBJECT + 1
    test = (row // len(DAYS)) % len(TESTS)
    visit = row % len(DAYS)
    value = _mix((number, 2654435761), (test, 97), (visit, 7919))
    missing = _mix((number, 40503), (test, 2654435761), (visit, 97)) % 25 == 0

    def by_test(position):
        return test.replace_strict(
            {index: entry[position] for index, entry in enumerate(TESTS)},
            return_dtype=pl.Float64 if position == 3 else pl.String,
        )

    return pl.DataFrame(
        {"r": pl.int_range(0, subjects * ROWS_PER_SUBJECT, eager=True)}
    ).select(
        pl.lit(STUDY).alias("STUDYID"),
        _subject_id(number).alias("USUBJID"),
        by_test(0).alias("LBTESTCD"),
        by_test(1).alias("LBTEST"),
        (
            _treatment_start(number)
            + pl.duration(
                days=visit.replace_strict(dict(enumerate(DAYS)), return_dtype=pl.Int64)
            )
        ).alias("LBDTC"),
        pl.when(missing & (visit != BASELINE_VISIT))
        .then(None)
        .otherwise(by_test(3) + (value % 400) / 10)
        .alias("LBSTRESN"),
        by_test(2).alias("LBSTRESU"),
    )


def vitals(rows):
    """`rows` subjects with a height in cm and a weight in kg, for BMI."""
    number = pl.col("n")
    return pl.DataFrame({"n": pl.int_range(1, rows + 1, eager=True)}).select(
        pl.lit(STUDY).alias("STUDYID"),
        pl.concat_str(pl.lit(f"{STUDY}-"), number.cast(pl.String).str.zfill(8)).alias(
            "USUBJID"
        ),
        (150 + (_mix((number, 7919)) % 500) / 10).alias("HEIGHTCM"),
        (45 + (_mix((number, 104729)) % 750) / 10).alias("WEIGHTKG"),
    )


def write(frame, path):
    """Write `frame` to `path` as Parquet, creating its directory."""
    path.parent.mkdir(parents=True, exist_ok=True)
    frame.write_parquet(path)
    return path
