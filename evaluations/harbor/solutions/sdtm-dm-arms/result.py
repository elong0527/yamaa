# Reference solution for the yamaa benchmark sdtm-dm-arms (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

rand = pl.read_csv("/app/input/rand.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)

# The planned arm follows randomization; the actual arm follows exposure.
# PLACEBO was the PBO arm and VITAMIN D3 the TRT arm.
planned = {"PLACEBO": ("PBO", "Placebo"), "VITAMIN D3": ("TRT", "Vitamin D3")}

exposure = (
    ex.group_by(["STUDYID", "USUBJID"])
    .agg(EXTRT=pl.col("EXTRT").filter(pl.col("EXTRT").fill_null("") != "").first())
    .with_columns(EXTRT=pl.col("EXTRT").fill_null(""))
)

subjects = (
    rand.select("STUDYID", "USUBJID", "RANDCD", "RAND", "SCRNFL")
    .join(exposure, on=["STUDYID", "USUBJID"], how="full", coalesce=True)
)

rows = []
for record in subjects.to_dicts():
    studyid = record["STUDYID"]
    usubjid = record["USUBJID"]
    randcd = (record.get("RANDCD") or "").strip() or None
    randdesc = (record.get("RAND") or "").strip() or None
    extrt = (record.get("EXTRT") or "").strip() or None
    scrnfl = (record.get("SCRNFL") or "").strip()
    # Never randomized leaves the planned arm empty; never treated or a
    # treatment matching no planned arm leaves the actual arm empty.
    armcd = randcd if randcd else None
    arm = randdesc if randcd else None
    actcd, act = planned.get(extrt, (None, None))
    # A treated subject carries the other planned arm when that is what
    # the exposure says; the reason for a blank arm depends on why both
    # are blank, or on never being treated.
    if arm is None and act is None:
        armnrs = "SCREEN FAILURE" if scrnfl == "Y" else "NOT ASSIGNED"
    elif arm is not None and act is None and extrt is None:
        armnrs = "NOT TREATED"
    else:
        armnrs = None
    armud = extrt if (extrt is not None and extrt not in planned) else None
    rows.append(
        {
            "DOMAIN": "DM",
            "STUDYID": studyid,
            "USUBJID": usubjid,
            "ARMCD": armcd,
            "ARM": arm,
            "ACTARMCD": actcd,
            "ACTARM": act,
            "ARMNRS": armnrs,
            "ACTARMUD": armud,
        }
    )

dm = pl.DataFrame(
    rows,
    schema={
        "DOMAIN": pl.String,
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "ARMCD": pl.String,
        "ARM": pl.String,
        "ACTARMCD": pl.String,
        "ACTARM": pl.String,
        "ARMNRS": pl.String,
        "ACTARMUD": pl.String,
    },
).sort("USUBJID")

Path("/app/output").mkdir(exist_ok=True)
dm.write_csv("/app/output/dm.csv")
