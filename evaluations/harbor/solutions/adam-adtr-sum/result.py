# Reference solution for the yamaa benchmark adam-adtr-sum (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADTR: one record per subject and scheduled tumor assessment.

Reads TR, TRVISIT and TU from /app/input and writes the ADaM ADTR
dataset (sum of target lesion diameters) to /app/output/adtr.csv.
"""

import polars as pl

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adtr.csv"

PARAMCD = "SDIAM"
PARAM = "Sum of Target Lesion Diameters (mm)"


def main() -> None:
    tr = pl.read_csv(f"{INPUT_DIR}/tr.csv", infer_schema=False)
    trvisit = pl.read_csv(f"{INPUT_DIR}/trvisit.csv", infer_schema=False)
    tu = pl.read_csv(f"{INPUT_DIR}/tu.csv", infer_schema=False)

    # Target longest-diameter records only.
    target = tr.filter(
        (pl.col("TRGRPID") == "TARGET") & (pl.col("TRTESTCD") == "LDIAM")
    ).with_columns(pl.col("TRSTRESN").cast(pl.Float64, strict=False))

    # Per subject and assessment: how many such records exist, how many
    # carry a result, and the sum of the measured diameters. AVAL stays
    # missing when nothing was measured; NMEAS stays missing when the
    # assessment has no target longest-diameter records at all.
    if target.height:
        measured = target.group_by(["STUDYID", "USUBJID", "AVISIT"]).agg(
            pl.len().alias("_NREC"),
            pl.col("TRSTRESN").count().alias("NMEAS"),
            pl.col("TRSTRESN").sum().alias("_SUM"),
        )
    else:
        measured = pl.DataFrame(
            schema={
                "STUDYID": pl.String,
                "USUBJID": pl.String,
                "AVISIT": pl.String,
                "_NREC": pl.UInt32,
                "NMEAS": pl.UInt32,
                "_SUM": pl.Float64,
            }
        )
    measured = measured.with_columns(
        pl.when(pl.col("NMEAS") > 0)
        .then(pl.col("_SUM"))
        .otherwise(None)
        .alias("AVAL"),
        pl.when(pl.col("_NREC") > 0)
        .then(pl.col("NMEAS"))
        .otherwise(None)
        .cast(pl.Int64)
        .alias("NMEAS"),
    ).drop(["_NREC", "_SUM"])

    # Number of target lesions in the subject's lesion inventory; the
    # same at every assessment of the subject.
    ntarget = (
        tu.filter(pl.col("TUGRPID") == "TARGET")
        .group_by(["STUDYID", "USUBJID"])
        .agg(pl.len().cast(pl.Int64).alias("NTARGET"))
    )

    adtr = (
        trvisit.join(
            measured, on=["STUDYID", "USUBJID", "AVISIT"], how="left"
        )
        .join(ntarget, on=["STUDYID", "USUBJID"], how="left")
        .with_columns(
            pl.col("NTARGET").fill_null(0),
            pl.col("AVISITN").cast(pl.Int64, strict=False),
            pl.lit(PARAMCD).alias("PARAMCD"),
            pl.lit(PARAM).alias("PARAM"),
            pl.when(
                pl.col("NMEAS").is_not_null()
                & (pl.col("NMEAS") == pl.col("NTARGET"))
            )
            .then(pl.lit("Y"))
            .otherwise(None)
            .alias("ANL01FL"),
        )
        .sort(["USUBJID", "AVISITN"])
        .select(
            [
                "STUDYID",
                "USUBJID",
                "AVISIT",
                "AVISITN",
                "ADT",
                "PARAMCD",
                "PARAM",
                "AVAL",
                "NMEAS",
                "NTARGET",
                "ANL01FL",
            ]
        )
    )

    # Write whole-number sums without a decimal part.
    aval = adtr["AVAL"].drop_nulls()
    if aval.len() and (aval == aval.cast(pl.Int64)).all():
        adtr = adtr.with_columns(pl.col("AVAL").cast(pl.Int64))

    adtr.write_csv(OUTPUT_PATH)


if __name__ == "__main__":
    main()
