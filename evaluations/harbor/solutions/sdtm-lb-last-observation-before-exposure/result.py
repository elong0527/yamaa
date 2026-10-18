# Reference solution for the yamaa benchmark sdtm-lb-last-observation-before-exposure (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the LB dataset from DM, LB_MAPPING and ODM extracts.

One output record per laboratory result item group in the ODM extract.
LBLOBXFL flags the last observation with a result, within each
(LBTESTCD, LBSPEC) series for a subject, whose collection date falls on
or before the subject's reference start date (RFSTDTC in DM). A subject
without a reference start date has the flag on the latest record with a
result in the series.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_FILE = "/app/output/lb.csv"

COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "LBSEQ",
    "LBTESTCD",
    "LBSPEC",
    "LBORRES",
    "LBDTC",
    "LBSTAT",
    "LBLOBXFL",
]


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def day(text):
    """Parse an ISO date/datetime at day precision; None when blank."""
    text = (text or "").strip()
    if not text:
        return None
    return date.fromisoformat(text[:10])


def main():
    dm = read_csv(f"{INPUT_DIR}/dm.csv")
    mapping = read_csv(f"{INPUT_DIR}/lb_mapping.csv")
    odm = read_csv(f"{INPUT_DIR}/odm.csv")

    ref_start = {}
    for row in dm:
        ref_start[row["USUBJID"]] = day(row["RFSTDTC"])

    test_map = {row["ItemOID"]: (row["LBTESTCD"], row["LBSPEC"]) for row in mapping}

    # Group the ODM items into one record per item group occurrence,
    # keeping the collected order.
    groups = {}
    order = []
    for row in odm:
        key = (row["StudyOID"], row["SubjectKey"], row["ItemGroupRepeatKey"])
        if key not in groups:
            groups[key] = {
                "STUDYID": row["StudyOID"],
                "USUBJID": row["SubjectKey"],
                "LBSEQ": row["ItemGroupRepeatKey"],
                "items": {},
            }
            order.append(key)
        groups[key]["items"][row["ItemOID"]] = row["Value"]

    records = []
    for key in order:
        group = groups[key]
        items = group["items"]
        result_items = [oid for oid in items if oid in test_map]
        if not result_items:
            continue
        item_oid = result_items[0]
        testcd, spec = test_map[item_oid]
        result = (items.get(item_oid) or "").strip()
        stat = (items.get("IT.LB.LBSTAT") or "").strip()
        if not result:
            stat = stat or "NOT DONE"
        records.append(
            {
                "DOMAIN": "LB",
                "STUDYID": group["STUDYID"],
                "USUBJID": group["USUBJID"],
                "LBSEQ": group["LBSEQ"],
                "LBTESTCD": testcd,
                "LBSPEC": spec,
                "LBORRES": result,
                "LBDTC": (items.get("IT.LB.LBDTC") or "").strip(),
                "LBSTAT": stat,
                "LBLOBXFL": "",
            }
        )

    # Assign the flag within each subject and (test, specimen) series.
    series = {}
    for idx, record in enumerate(records):
        key = (record["USUBJID"], record["LBTESTCD"], record["LBSPEC"])
        series.setdefault(key, []).append(idx)

    for (usubjid, _testcd, _spec), idxs in series.items():
        ref = ref_start.get(usubjid)
        candidates = []
        for idx in idxs:
            record = records[idx]
            if not record["LBORRES"]:
                continue  # a record without a result never carries the flag
            collected = day(record["LBDTC"])
            if ref is not None and (collected is None or collected > ref):
                continue  # collected after the reference start date
            candidates.append(idx)
        if not candidates:
            continue
        best = max(
            candidates,
            key=lambda i: (day(records[i]["LBDTC"]) or date.min, int(records[i]["LBSEQ"])),
        )
        records[best]["LBLOBXFL"] = "Y"

    with open(OUTPUT_FILE, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
