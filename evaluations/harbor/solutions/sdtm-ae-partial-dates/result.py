# Reference solution for the yamaa benchmark sdtm-ae-partial-dates (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM AE dataset from DM and ODM long-format inputs.

One record per adverse event (one AE form repeat per subject). Start and
end dates are assembled from their separately collected year, month, and
day items and written as ISO 8601 text at the precision collected; nothing
is imputed. Study days are computed only for complete dates and only when
the subject has a reference start date (RFSTDTC) in DM.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/ae.csv"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "AESEQ",
    "AETERM",
    "AESTDTC",
    "AEENDTC",
    "AESTDY",
    "AEENDY",
]


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def assemble_date(parts, prefix):
    """Join collected year/month/day items at the precision collected."""
    year = parts.get(prefix + "YR", "").strip()
    if not year:
        return ""
    month = parts.get(prefix + "MO", "").strip()
    if not month:
        return year
    day = parts.get(prefix + "DY", "").strip()
    if not day:
        return f"{year}-{int(month):02d}"
    return f"{year}-{int(month):02d}-{int(day):02d}"


def parse_full_date(text):
    """Return a date for a complete ISO date string, else None."""
    if len(text) != 10:
        return None
    try:
        year, month, day = (int(piece) for piece in text.split("-"))
        return date(year, month, day)
    except ValueError:
        return None


def study_day(date_text, ref_start):
    """SDTM study day of a complete date relative to the reference start.

    The reference start date itself is day 1; the day before it is day -1
    (there is no day 0). Partial dates and missing reference dates have
    no study day.
    """
    value = parse_full_date(date_text)
    if value is None or ref_start is None:
        return ""
    delta = (value - ref_start).days
    return str(delta + 1 if delta >= 0 else delta)


def main():
    dm_rows = read_csv(f"{INPUT_DIR}/dm.csv")
    ref_start_by_subject = {}
    for row in dm_rows:
        ref = parse_full_date((row.get("RFSTDTC") or "").strip())
        ref_start_by_subject[row["USUBJID"]] = ref

    odm_rows = read_csv(f"{INPUT_DIR}/odm.csv")

    # Group the AE form items by subject and form repeat key (AESEQ),
    # keeping first-appearance order of subjects and events.
    events = {}
    order = []
    for row in odm_rows:
        if row["FormOID"] != "FO.AE":
            continue
        key = (row["StudyOID"], row["SubjectKey"], row["FormRepeatKey"])
        if key not in events:
            events[key] = {}
            order.append(key)
        item = row["ItemOID"].rsplit(".", 1)[-1]
        events[key][item] = row["Value"]

    out_rows = []
    for study_oid, subject_key, repeat_key in order:
        items = events[(study_oid, subject_key, repeat_key)]
        start_dtc = assemble_date(items, "AEST")
        end_dtc = assemble_date(items, "AEEN")
        ref_start = ref_start_by_subject.get(subject_key)
        out_rows.append(
            {
                "DOMAIN": "AE",
                "STUDYID": study_oid,
                "USUBJID": subject_key,
                "AESEQ": repeat_key,
                "AETERM": items.get("AETERM", ""),
                "AESTDTC": start_dtc,
                "AEENDTC": end_dtc,
                "AESTDY": study_day(start_dtc, ref_start),
                "AEENDY": study_day(end_dtc, ref_start),
            }
        )

    with open(OUTPUT_PATH, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
