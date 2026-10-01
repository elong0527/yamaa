# Reference solution for the yamaa benchmark sdtm-relrec-dataset-level (Python track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value).strip() != ""


ae = pl.read_csv("/app/input/ae.csv", infer_schema=False)
cm = pl.read_csv("/app/input/cm.csv", infer_schema=False)
tr = pl.read_csv("/app/input/tr.csv", infer_schema=False)
tu = pl.read_csv("/app/input/tu.csv", infer_schema=False)

studyid = str(ae["STUDYID"][0]) if len(ae) else str(tr["STUDYID"][0])

rows = [
    # Whole domains rather than subject records: one tumor identification
    # relates to many tumor results.
    {
        "STUDYID": studyid,
        "USUBJID": None,
        "RDOMAIN": "TU",
        "IDVAR": "TULNKID",
        "IDVARVAL": None,
        "RELTYPE": "ONE",
        "RELID": "1",
    },
    {
        "STUDYID": studyid,
        "USUBJID": None,
        "RDOMAIN": "TR",
        "IDVAR": "TRLNKID",
        "IDVARVAL": None,
        "RELTYPE": "MANY",
        "RELID": "1",
    },
]

for row in ae.to_dicts():
    if not _present(row.get("AELNKID")):
        continue
    rows.append(
        {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "RDOMAIN": "AE",
            "IDVAR": "AESEQ",
            "IDVARVAL": str(row["AESEQ"]).strip(),
            "RELTYPE": None,
            "RELID": str(row["AELNKID"]).strip(),
        }
    )
for row in cm.to_dicts():
    if not _present(row.get("CMLNKID")):
        continue
    rows.append(
        {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "RDOMAIN": "CM",
            "IDVAR": "CMSEQ",
            "IDVARVAL": str(row["CMSEQ"]).strip(),
            "RELTYPE": None,
            "RELID": str(row["CMLNKID"]).strip(),
        }
    )

relrec = pl.DataFrame(
    rows,
    schema={
        "STUDYID": pl.String,
        "USUBJID": pl.String,
        "RDOMAIN": pl.String,
        "IDVAR": pl.String,
        "IDVARVAL": pl.String,
        "RELTYPE": pl.String,
        "RELID": pl.String,
    },
).select("STUDYID", "USUBJID", "RDOMAIN", "IDVAR", "IDVARVAL", "RELTYPE", "RELID")

Path("/app/output").mkdir(exist_ok=True)
relrec.write_csv("/app/output/relrec.csv")
