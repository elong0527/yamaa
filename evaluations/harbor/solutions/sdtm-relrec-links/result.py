# Reference solution for the yamaa benchmark sdtm-relrec-links (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from pathlib import Path

import polars as pl


def _present(value) -> bool:
    return value is not None and str(value).strip() != ""


def _link(value) -> str | None:
    """A collected link number as text without a decimal point."""
    if not _present(value):
        return None
    text = str(value).strip()
    try:
        number = float(text)
        if number.is_integer():
            return str(int(number))
        return str(number)
    except ValueError:
        return text


ae = pl.read_csv("/app/input/ae.csv", infer_schema=False)
cm = pl.read_csv("/app/input/cm.csv", infer_schema=False)

rows = []
for row in ae.to_dicts():
    for link_col in ("AELNKID1", "AELNKID2"):
        relid = _link(row.get(link_col))
        # A record with no link identifier contributes no row.
        if relid is None:
            continue
        rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "RDOMAIN": "AE",
                "IDVAR": "AESEQ",
                "IDVARVAL": str(row["AESEQ"]).strip(),
                "RELTYPE": None,
                "RELID": relid,
            }
        )
for row in cm.to_dicts():
    for link_col in ("CMLNKID1", "CMLNKID2"):
        relid = _link(row.get(link_col))
        if relid is None:
            continue
        rows.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "RDOMAIN": "CM",
                "IDVAR": "CMSEQ",
                "IDVARVAL": str(row["CMSEQ"]).strip(),
                "RELTYPE": None,
                "RELID": relid,
            }
        )

relrec = (
    pl.DataFrame(
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
    )
    .with_columns(
        pl.col("IDVARVAL").cast(pl.Int64).alias("_seq"),
        pl.col("RELID").cast(pl.Int64).alias("_rel"),
    )
    .sort(["USUBJID", "_rel", "RDOMAIN", "_seq"])
    .select("STUDYID", "USUBJID", "RDOMAIN", "IDVAR", "IDVARVAL", "RELTYPE", "RELID")
)

Path("/app/output").mkdir(exist_ok=True)
relrec.write_csv("/app/output/relrec.csv")
