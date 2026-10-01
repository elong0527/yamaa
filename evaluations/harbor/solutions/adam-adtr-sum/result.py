# Reference solution for the yamaa benchmark adam-adtr-sum (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

trvisit = pl.read_csv("/app/input/trvisit.csv", infer_schema=False, null_values="").with_columns(
    pl.col("AVISITN").cast(pl.Int64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
)
tr = pl.read_csv("/app/input/tr.csv", infer_schema=False, null_values="").with_columns(
    pl.col("TRSTRESN").cast(pl.Float64, strict=False),
)
tu = pl.read_csv("/app/input/tu.csv", infer_schema=False, null_values="")

# Lesions selected as target at study entry: the same count at every
# assessment of the subject.
ntarget = (
    tu.filter(pl.col("TUGRPID") == "TARGET")
    .group_by(["STUDYID", "USUBJID"])
    .agg(NTARGET=pl.len())
)

# Target longest diameters at each assessment. A record with no result
# contributes nothing, while a zero counts as measured.
agg = (
    tr.filter((pl.col("TRGRPID") == "TARGET") & (pl.col("TRTESTCD") == "LDIAM"))
    .group_by(["STUDYID", "USUBJID", "AVISIT"])
    .agg(
        __sum=pl.col("TRSTRESN").sum(),
        __cnt=pl.col("TRSTRESN").count(),
    )
)

adtr = (
    trvisit.join(agg, on=["STUDYID", "USUBJID", "AVISIT"], how="left", maintain_order="left")
    .join(ntarget, on=["STUDYID", "USUBJID"], how="left")
    .with_columns(
        AVAL=pl.when(pl.col("__cnt") == 0)
        .then(None)
        .otherwise(pl.col("__sum")),
        NMEAS=pl.col("__cnt"),
        PARAMCD=pl.lit("SDIAM"),
        PARAM=pl.lit("Sum of Target Lesion Diameters (mm)"),
    )
    .with_columns(
        ANL01FL=pl.when(
            pl.col("NMEAS").is_not_null()
            & pl.col("NTARGET").is_not_null()
            & (pl.col("NMEAS") == pl.col("NTARGET"))
        )
        .then(pl.lit("Y"))
        .otherwise(None),
    )
    .select(
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
    )
    .sort(["USUBJID", "AVISITN"])
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adtr.write_csv("/app/output/adtr.csv", null_value="")
