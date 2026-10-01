# Reference solution for the yamaa benchmark adam-adtte-os (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl = pl.read_csv("/app/input/adsl.csv", infer_schema=False, null_values="").with_columns(
    pl.col("RANDDT").str.to_date(strict=False),
    pl.col("LSTALVDT").str.to_date(strict=False),
)
adrs = pl.read_csv("/app/input/adrs.csv", infer_schema=False, null_values="").with_columns(
    pl.col("ASEQ").cast(pl.Int64, strict=False),
    pl.col("ADT").str.to_date(strict=False),
)

# The earliest dated qualifying death: code DEATH, result Y, flag Y.
# On a tied date the lower sequence wins.
death = (
    adrs.filter(
        (pl.col("PARAMCD") == "DEATH")
        & (pl.col("AVALC") == "Y")
        & (pl.col("ANL01FL") == "Y")
        & pl.col("ADT").is_not_null()
    )
    .sort(["STUDYID", "USUBJID", "ADT", "ASEQ"])
    .unique(["STUDYID", "USUBJID"], keep="first", maintain_order=True)
    .select("STUDYID", "USUBJID", DEATHDT="ADT", DEATHSEQ="ASEQ")
)

adtte = (
    adsl.join(death, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(
        STARTDT=pl.col("RANDDT"),
        CENSORDT1=pl.col("LSTALVDT"),
    )
    .with_columns(
        # A death before randomization still counts, moved up to it; with
        # no death, the last-alive date when after randomization, else
        # randomization itself.
        ADT=pl.when(pl.col("DEATHDT") >= pl.col("STARTDT"))
        .then(pl.col("DEATHDT"))
        .when(pl.col("DEATHDT").is_not_null())
        .then(pl.col("STARTDT"))
        .when(pl.col("CENSORDT1") > pl.col("STARTDT"))
        .then(pl.col("CENSORDT1"))
        .otherwise(pl.col("STARTDT")),
    )
    .with_columns(
        AVAL=(pl.col("ADT") - pl.col("STARTDT")).dt.total_days() + 1,
        CNSR=pl.when(pl.col("DEATHDT").is_not_null()).then(0).otherwise(1),
        EVNTDESC=pl.when(pl.col("DEATHDT").is_not_null())
        .then(pl.lit("Death"))
        .when(pl.col("CENSORDT1") > pl.col("STARTDT"))
        .then(pl.lit("Alive"))
        .otherwise(pl.lit("Randomization")),
        CNSDTDSC=pl.when(pl.col("DEATHDT").is_not_null())
        .then(None)
        .when(pl.col("CENSORDT1") > pl.col("STARTDT"))
        .then(pl.lit("Alive During Study"))
        .otherwise(pl.lit("Randomization")),
        SRCDOM=pl.when(pl.col("DEATHDT").is_not_null())
        .then(pl.lit("ADRS"))
        .otherwise(pl.lit("ADSL")),
        SRCVAR=pl.when(pl.col("DEATHDT").is_not_null())
        .then(pl.lit("ADT"))
        .when(pl.col("CENSORDT1") > pl.col("STARTDT"))
        .then(pl.lit("LSTALVDT"))
        .otherwise(pl.lit("RANDDT")),
        SRCSEQ=pl.when(pl.col("DEATHDT").is_not_null())
        .then(pl.col("DEATHSEQ"))
        .otherwise(None),
        PARAMCD=pl.lit("OS"),
        PARAM=pl.lit("Overall Survival"),
    )
    .select(
        "STUDYID",
        "USUBJID",
        "PARAMCD",
        "PARAM",
        "STARTDT",
        "ADT",
        "AVAL",
        "CNSR",
        "EVNTDESC",
        "CNSDTDSC",
        "SRCDOM",
        "SRCVAR",
        "SRCSEQ",
    )
    .sort("USUBJID")
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
adtte.write_csv("/app/output/adtte.csv", null_value="")
