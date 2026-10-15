# Reference solution for the yamaa benchmark adam-adsl-period-dates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

from datetime import date, timedelta
from pathlib import Path

import polars as pl

dm = pl.read_csv("/app/input/dm.csv", infer_schema=False)
ex = pl.read_csv("/app/input/ex.csv", infer_schema=False)

starts: dict[tuple[str, str], list[date]] = {}
ends: dict[str, list[date]] = {}
for record in ex.iter_rows(named=True):
    if record["EXSTDTC"] is not None:
        starts.setdefault((record["USUBJID"], record["EPOCH"]), []).append(
            date.fromisoformat(record["EXSTDTC"])
        )
    # The last exposure end counts every record, with or without a start.
    if record["EXENDTC"] is not None:
        ends.setdefault(record["USUBJID"], []).append(date.fromisoformat(record["EXENDTC"]))


def text(value: date | None) -> str | None:
    return None if value is None else value.isoformat()


rows = []
for record in dm.iter_rows(named=True):
    subject = record["USUBJID"]
    ap01sdt = min(starts.get((subject, "TREATMENT 1"), []), default=None)
    ap02sdt = min(starts.get((subject, "TREATMENT 2"), []), default=None)
    last_end = max(ends.get(subject, []), default=None)
    follow_up = last_end + timedelta(days=30) if last_end else None
    # Period one ends the day before period two starts; the last period a
    # subject entered ends 30 days after the last exposure end.
    if ap02sdt is not None:
        ap01edt = ap02sdt - timedelta(days=1) if last_end else None
        ap02edt = follow_up
    else:
        ap01edt = follow_up
        ap02edt = None
    rows.append(
        {
            "STUDYID": record["STUDYID"],
            "USUBJID": subject,
            "AP01SDT": text(ap01sdt),
            "AP01EDT": text(ap01edt),
            "AP02SDT": text(ap02sdt),
            "AP02EDT": text(ap02edt),
        }
    )

adsl = pl.DataFrame(
    rows,
    schema={name: pl.String for name in ["STUDYID", "USUBJID", "AP01SDT", "AP01EDT", "AP02SDT", "AP02EDT"]},
)

Path("/app/output").mkdir(exist_ok=True)
adsl.write_csv("/app/output/adsl.csv")
