# Reference solution for the yamaa benchmark adam-adsl-alive-date (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

adsl_raw = pl.read_csv("/app/input/adsl.csv", infer_schema=False).with_columns(
    pl.col("TRTEDT").str.to_date(strict=False)
)
adae = pl.read_csv("/app/input/adae.csv", infer_schema=False).with_columns(
    pl.col("AENDT").str.to_date(strict=False)
)
advs = pl.read_csv("/app/input/advs.csv", infer_schema=False).with_columns(
    pl.col("ADATE").str.to_date(strict=False)
)

# The contact text completed to a day: a full date stands, a year and month
# take the month's first day, a year alone takes January first, and anything
# else leaves the date missing.
adsl_raw = adsl_raw.with_columns(
    LSTCNTDT=pl.coalesce(
        pl.col("LSTCNTDC").str.to_date(strict=False),
        (pl.col("LSTCNTDC") + "-01").str.to_date(strict=False),
        (pl.col("LSTCNTDC") + "-01-01").str.to_date(strict=False),
    )
)

# Each subject's latest adverse event end and vital signs dates.
adae_last = adae.group_by(["STUDYID", "USUBJID"]).agg(
    ADAE_LST=pl.col("AENDT").max()
)
advs_last = advs.group_by(["STUDYID", "USUBJID"]).agg(
    ADVS_LST=pl.col("ADATE").max()
)

adsl = (
    adsl_raw.join(adae_last, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .join(advs_last, on=["STUDYID", "USUBJID"], how="left", maintain_order="left")
    .with_columns(
        LSTALVDT=pl.max_horizontal("TRTEDT", "LSTCNTDT", "ADAE_LST", "ADVS_LST")
    )
    .select("STUDYID", "USUBJID", "TRTEDT", "LSTCNTDC", "LSTCNTDT", "LSTALVDT")
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
