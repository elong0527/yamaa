# Reference solution for the yamaa benchmark sdtm-fa-reactogenicity-diary (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl

diary = pl.read_csv("/app/input/diary.csv", infer_schema=False).with_columns(
    pl.col("DIARYDAY").cast(pl.Int64),
)

test_order = {"OCCUR": 0, "SEV": 1, "LDIAM": 2}
test_name = {
    "OCCUR": "Occurrence Indicator",
    "SEV": "Severity/Intensity",
    "LDIAM": "Longest Diameter",
}
local_reactions = {"PAIN", "REDNESS", "SWELLING"}


def is_local(reaction, catsrc):
    # Local reactions map to ADMINISTRATION SITE; systemic stay SYSTEMIC.
    if catsrc == "LOCAL":
        return True
    if catsrc == "SYSTEMIC":
        return False
    return reaction in local_reactions


rows = []
for r in diary.to_dicts():
    study, subj = r["STUDYID"], r["USUBJID"]
    day, tpt, dtc = r["DIARYDAY"], r["TPT"], r["DTC"]
    reaction = r["REACTION"]
    completed = r["COMPLETED"]
    missed = completed != "Y"
    fascat = "ADMINISTRATION SITE" if is_local(reaction, r["CATSRC"]) else "SYSTEMIC"
    base = {
        "DOMAIN": "FA",
        "STUDYID": study,
        "USUBJID": subj,
        "DAY": day,
        "REACTION": reaction,
        "FAOBJ": reaction,
        "FACAT": "REACTOGENICITY",
        "FASCAT": fascat,
        "FATPT": tpt,
        "FADTC": dtc,
    }
    if missed:
        rows.append(
            {
                **base,
                "ORDER": 0,
                "FATESTCD": "OCCUR",
                "FATEST": test_name["OCCUR"],
                "FAORRES": None,
                "FAORRESU": None,
                "FASTRESC": None,
                "FASTRESN": None,
                "FASTRESU": None,
                "FASTAT": "NOT DONE",
            }
        )
        continue
    occur = r["OCCUR"] if r["OCCUR"] not in (None, "") else None
    sev = r["SEV"] if r["SEV"] not in (None, "") else None
    diam = r["DIAMETER"] if r["DIAMETER"] not in (None, "") else None
    unit = r["DIAMUNIT"] if r["DIAMUNIT"] not in (None, "") else None
    rows.append(
        {
            **base,
            "ORDER": 0,
            "FATESTCD": "OCCUR",
            "FATEST": test_name["OCCUR"],
            "FAORRES": occur,
            "FAORRESU": None,
            "FASTRESC": occur,
            "FASTRESN": None,
            "FASTRESU": None,
            "FASTAT": None,
        }
    )
    rows.append(
        {
            **base,
            "ORDER": 1,
            "FATESTCD": "SEV",
            "FATEST": test_name["SEV"],
            "FAORRES": sev,
            "FAORRESU": None,
            "FASTRESC": sev,
            "FASTRESN": None,
            "FASTRESU": None,
            "FASTAT": None,
        }
    )
    if reaction in ("REDNESS", "SWELLING") and occur == "Y":
        try:
            num = float(diam) if diam is not None else None
        except ValueError:
            num = None
        rows.append(
            {
                **base,
                "ORDER": 2,
                "FATESTCD": "LDIAM",
                "FATEST": test_name["LDIAM"],
                "FAORRES": diam,
                "FAORRESU": unit,
                "FASTRESC": diam,
                "FASTRESN": num,
                "FASTRESU": unit,
                "FASTAT": None,
            }
        )

fa = (
    pl.DataFrame(
        rows,
        schema={
            "DOMAIN": pl.String,
            "STUDYID": pl.String,
            "USUBJID": pl.String,
            "DAY": pl.Int64,
            "REACTION": pl.String,
            "FAOBJ": pl.String,
            "FACAT": pl.String,
            "FASCAT": pl.String,
            "FATPT": pl.String,
            "FADTC": pl.String,
            "ORDER": pl.Int64,
            "FATESTCD": pl.String,
            "FATEST": pl.String,
            "FAORRES": pl.String,
            "FAORRESU": pl.String,
            "FASTRESC": pl.String,
            "FASTRESN": pl.Float64,
            "FASTRESU": pl.String,
            "FASTAT": pl.String,
        },
    )
    .sort(["STUDYID", "USUBJID", "DAY", "REACTION", "ORDER"])
    .with_columns(
        pl.int_range(1, pl.len() + 1).over(["STUDYID", "USUBJID"]).alias("FASEQ")
    )
    .select(
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "FASEQ",
        "FATESTCD",
        "FATEST",
        "FAOBJ",
        "FACAT",
        "FASCAT",
        "FAORRES",
        "FAORRESU",
        "FASTRESC",
        "FASTRESN",
        "FASTRESU",
        "FASTAT",
        "FATPT",
        "FADTC",
    )
)

Path("/app/output").mkdir(parents=True, exist_ok=True)
fa.write_csv("/app/output/fa.csv")
