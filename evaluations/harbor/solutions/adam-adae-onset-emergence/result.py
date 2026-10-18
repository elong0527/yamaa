# Reference solution for the yamaa benchmark adam-adae-onset-emergence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create ADAE (one record per adverse event) from ADSL and AE.

Treatment emergence is decided at second precision: an onset collected
without seconds has its seconds taken as zero. TRTEMFL is "Y" when the
onset moment is at or after the first exposure moment and at or before
the end of the treatment-emergent window (30 days after last exposure).
"""

import csv
from datetime import datetime, timedelta

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_dtm(value):
    """Parse an ISO datetime; missing seconds are taken as zero."""
    value = (value or "").strip()
    if not value:
        return None
    for fmt in ("%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d"):
        try:
            return datetime.strptime(value, fmt)
        except ValueError:
            continue
    raise ValueError(f"Unrecognised datetime value: {value!r}")


def format_dtm(value):
    if value is None:
        return ""
    return value.strftime("%Y-%m-%dT%H:%M:%S")


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main():
    adsl = read_csv(f"{INPUT_DIR}/adsl.csv")
    ae = read_csv(f"{INPUT_DIR}/ae.csv")

    subjects = {}
    for row in adsl:
        trtsdtm = parse_dtm(row.get("TRTSDTM"))
        trtendtm = parse_dtm(row.get("TRTENDTM"))
        teenddtm = parse_dtm(row.get("TEENDDTM"))
        if teenddtm is None and trtendtm is not None:
            teenddtm = trtendtm + timedelta(days=30)
        subjects[(row["STUDYID"], row["USUBJID"])] = (trtsdtm, teenddtm)

    records = []
    for row in ae:
        key = (row["STUDYID"], row["USUBJID"])
        trtsdtm, teenddtm = subjects.get(key, (None, None))
        astdtm = parse_dtm(row.get("AESTDTC"))

        if trtsdtm is None:
            trtemfl = ""
        elif astdtm is None:
            trtemfl = "Y"
        elif astdtm < trtsdtm:
            trtemfl = ""
        elif teenddtm is not None and astdtm > teenddtm:
            trtemfl = ""
        else:
            trtemfl = "Y"

        records.append(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "AESEQ": row["AESEQ"],
                "AETERM": row["AETERM"],
                "ASTDTM": astdtm,
                "TRTSDTM": trtsdtm,
                "TEENDDTM": teenddtm,
                "TRTEMFL": trtemfl,
                "AOCCFL": "",
            }
        )

    # AOCCFL flags each subject's earliest treatment-emergent event by
    # onset moment; ties on the same onset second go to the lower AESEQ.
    # Events without an onset moment never carry AOCCFL.
    earliest = {}
    for record in records:
        if record["TRTEMFL"] != "Y" or record["ASTDTM"] is None:
            continue
        key = (record["STUDYID"], record["USUBJID"])
        candidate = (record["ASTDTM"], int(record["AESEQ"]))
        if key not in earliest or candidate < earliest[key][0]:
            earliest[key] = (candidate, record)
    for _, record in earliest.values():
        record["AOCCFL"] = "Y"

    columns = [
        "STUDYID",
        "USUBJID",
        "AESEQ",
        "AETERM",
        "ASTDTM",
        "TRTSDTM",
        "TEENDDTM",
        "TRTEMFL",
        "AOCCFL",
    ]
    with open(f"{OUTPUT_DIR}/adae.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        for record in records:
            out = dict(record)
            out["ASTDTM"] = format_dtm(record["ASTDTM"])
            out["TRTSDTM"] = format_dtm(record["TRTSDTM"])
            out["TEENDDTM"] = format_dtm(record["TEENDDTM"])
            writer.writerow(out)


if __name__ == "__main__":
    main()
