# Reference solution for the yamaa benchmark adam-adsl-randomization (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create the ADSL randomization dataset from the DM and ODM datasets.

One record per subject with columns:
STUDYID, USUBJID, RANDDT, RANDDY, RANDDTC

- RANDDT is the subject's randomization date from DM.
- RANDDY is the study day of RANDDT measured from RFSTDTC (the subject's
  reference start date): there is no study day 0, so a randomization on
  RFSTDTC is day 1 and the day before RFSTDTC is day -1. RANDDY has no
  value when RANDDT or RFSTDTC has no value.
- RANDDTC is the collected randomization moment RANDDTTM written as text
  at whole-second precision (date and time joined by T, seconds always
  shown). It is written independently of the collected date, so a
  subject with a collected moment but no collected date still has
  RANDDTC while RANDDT and RANDDY have no value.
"""

import polars as pl

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def check_odm_agrees_with_dm(dm: pl.DataFrame) -> None:
    """Confirm the ODM collected DM values agree with the DM dataset."""
    odm = pl.read_csv(f"{INPUT_DIR}/odm.csv", infer_schema=False)
    items = odm.filter(pl.col("ItemGroupOID") == "IG.DM").pivot(
        values="Value", index="SubjectKey", on="ItemOID"
    )
    for item, column, fmt in (
        ("IT.DM.RFSTDTC", "RFSTDTC", "%Y-%m-%d"),
        ("IT.DM.RANDDT", "RANDDT", "%Y-%m-%d"),
        ("IT.DM.RANDDTTM", "RANDDTTM", "%Y-%m-%d %H:%M:%S"),
    ):
        if item not in items.columns:
            if dm[column].is_not_null().any():
                raise ValueError(f"ODM has no {item} but DM has {column}")
            continue
        collected = items.select(
            pl.col("SubjectKey").alias("USUBJID"),
            pl.col(item).alias("collected"),
        )
        joined = dm.join(collected, on="USUBJID", how="left")
        typed_text = joined[column].dt.strftime(fmt)
        collected_text = joined["collected"]
        if column == "RANDDTTM":
            # Collected moments may carry only minute precision
            # ("2024-03-04T10:30"); normalize to whole seconds.
            collected_text = (
                pl.when(collected_text.str.len_chars() == 16)
                .then(collected_text.str.replace("T", " ") + ":00")
                .otherwise(collected_text.str.replace("T", " "))
            )
        mismatch = joined.filter(
            typed_text.is_not_null()
            & collected_text.is_not_null()
            & (typed_text != collected_text)
        )
        if mismatch.height:
            raise ValueError(
                f"ODM collected {column} disagrees with DM for "
                f"{mismatch['USUBJID'].to_list()}"
            )


def main() -> None:
    dm = pl.read_parquet(f"{INPUT_DIR}/dm.parquet")
    check_odm_agrees_with_dm(dm)

    diff_days = (pl.col("RANDDT") - pl.col("RFSTDTC")).dt.total_days()
    adsl = dm.select(
        "STUDYID",
        "USUBJID",
        "RANDDT",
        pl.when(pl.col("RANDDT").is_null() | pl.col("RFSTDTC").is_null())
        .then(None)
        .when(diff_days >= 0)
        .then(diff_days + 1)
        .otherwise(diff_days)
        .alias("RANDDY"),
        pl.col("RANDDTTM").dt.strftime("%Y-%m-%dT%H:%M:%S").alias("RANDDTC"),
    )

    adsl.write_csv(f"{OUTPUT_DIR}/adsl.csv")


if __name__ == "__main__":
    main()
