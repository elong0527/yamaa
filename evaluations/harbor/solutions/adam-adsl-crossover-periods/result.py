# Reference solution for the yamaa benchmark adam-adsl-crossover-periods (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False).with_columns(
    pl.col("EXSEQ").cast(pl.Int64, strict=False),
    pl.col("EXSTDTC").str.to_date(strict=False),
    pl.col("EXENDTC").str.to_date(strict=False),
)

ex1 = ex.filter(pl.col("EPOCH") == "TREATMENT 1")
ex2 = ex.filter(pl.col("EPOCH") == "TREATMENT 2")

agg1 = ex1.group_by(["STUDYID", "USUBJID"]).agg(
    TR01SDT=pl.col("EXSTDTC").min(), TR01EDT=pl.col("EXENDTC").max()
)
agg2 = ex2.group_by(["STUDYID", "USUBJID"]).agg(
    TR02SDT=pl.col("EXSTDTC").min(), TR02EDT=pl.col("EXENDTC").max()
)

# The treatment on the earliest record in each period: earliest start date
# with the lower sequence number breaking ties, records without a start
# date sorting last.
first1 = (
    ex1.sort(["EXSTDTC", "EXSEQ"], nulls_last=True)
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", TRT01A="EXTRT")
)
first2 = (
    ex2.sort(["EXSTDTC", "EXSEQ"], nulls_last=True)
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", TRT02A="EXTRT")
)

adsl = (
    dm.join(agg1, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .join(agg2, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .join(first1, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .join(first2, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    # Days strictly between the periods, counting neither endpoint.
    .with_columns(
        WASHDUR=(pl.col("TR02SDT") - pl.col("TR01EDT")).dt.total_days() - 1
    )
    .select(
        "STUDYID",
        "USUBJID",
        "TR01SDT",
        "TR01EDT",
        "TR02SDT",
        "TR02EDT",
        "TRT01A",
        "TRT02A",
        "WASHDUR",
    )
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
