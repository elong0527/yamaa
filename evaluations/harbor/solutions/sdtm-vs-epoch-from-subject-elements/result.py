# Reference solution for the yamaa benchmark sdtm-vs-epoch-from-subject-elements (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import datetime
from pathlib import Path

import polars as pl


def parse_moment(text: str | None) -> datetime | None:
    if text is None or text == "":
        return None
    # A partial date such as YYYY-MM has no day, so it cannot be placed in
    # an element.
    if len(text) == 7 and text[4] == "-":
        return None
    if len(text) == 4 and text.isdigit():
        return None
    try:
        # A date-only collection reads as the start of that day.
        if "T" not in text:
            return datetime.fromisoformat(text + "T00:00:00")
        return datetime.fromisoformat(text)
    except ValueError:
        return None


se = pl.read_csv("/app/input/se.csv", infer_schema=False)
vs_raw = pl.read_csv("/app/input/vs_raw.csv", infer_schema=False).with_columns(
    pl.col("VSSEQ").cast(pl.Int64)
)

elements: dict[str, list[dict]] = {}
for row in se.to_dicts():
    elements.setdefault(row["USUBJID"], []).append(
        {
            "EPOCH": row["EPOCH"],
            "start": parse_moment(row["SESTDTC"]),
            "end": parse_moment(row["SEENDTC"]),
        }
    )

rows = []
for row in vs_raw.to_dicts():
    coll = parse_moment(row["VSDTC"])
    # Only a full date or datetime can fall in an element; empty and
    # partial dates stay empty, as do readings outside every element.
    epoch = None
    if coll is not None:
        hits = [
            e
            for e in elements.get(row["USUBJID"], [])
            if e["start"] is not None
            and e["end"] is not None
            and e["start"] <= coll <= e["end"]
        ]
        if hits:
            hits.sort(key=lambda e: e["start"])
            # A reading exactly at a shared boundary belongs to the later
            # element.
            epoch = hits[-1]["EPOCH"]
    rows.append(
        {
            "DOMAIN": "VS",
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "VSSEQ": row["VSSEQ"],
            "VSTESTCD": row["VSTESTCD"],
            "VSORRES": row["VSORRES"],
            "VSDTC": row["VSDTC"],
            "EPOCH": epoch,
        }
    )

vs = pl.DataFrame(rows).sort(["USUBJID", "VSSEQ"]).select(
    "DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VSTESTCD", "VSORRES", "VSDTC", "EPOCH"
)

Path("/app/output").mkdir(exist_ok=True)
vs.write_csv("/app/output/vs.csv")
