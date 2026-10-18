# Reference solution for the yamaa benchmark sdtm-vs-study-day (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def parse_date(value):
    value = (value or "").strip()
    if not value:
        return None
    return date.fromisoformat(value)


def format_number(value):
    if value is None:
        return ""
    if float(value).is_integer():
        return str(int(value))
    return f"{value:.2f}".rstrip("0").rstrip(".")


def main():
    dm = read_csv(f"{INPUT_DIR}/dm.csv")
    se = read_csv(f"{INPUT_DIR}/se.csv")
    tv = read_csv(f"{INPUT_DIR}/tv.csv")
    vs_raw = read_csv(f"{INPUT_DIR}/vs_raw.csv")

    ref_start = {}
    for row in dm:
        ref_start[row["USUBJID"]] = parse_date(row["RFSTDTC"])

    elements = {}
    for row in se:
        elements.setdefault(row["USUBJID"], []).append(
            (row["EPOCH"], parse_date(row["SESTDTC"]), parse_date(row["SEENDTC"]))
        )

    planned = {}
    for row in tv:
        planned[row["VISIT"]] = float(row["VISITNUM"])

    def unplanned_visitnum(subject, coll_date):
        # Latest planned visit number collected for the subject before
        # the record's collection date, plus 0.01.
        if coll_date is None:
            return None
        best = None
        for row in vs_raw:
            if row["USUBJID"] != subject or row["VISIT"] not in planned:
                continue
            other_date = parse_date(row["VSDTC"])
            if other_date is not None and other_date < coll_date:
                num = planned[row["VISIT"]]
                if best is None or num > best:
                    best = num
        return None if best is None else best + 0.01

    out_rows = []
    for row in vs_raw:
        subject = row["USUBJID"]
        coll_date = parse_date(row["VSDTC"])

        if row["VISIT"] in planned:
            visitnum = planned[row["VISIT"]]
        else:
            visitnum = unplanned_visitnum(subject, coll_date)

        epoch = ""
        if coll_date is not None:
            chosen = None
            for name, start, end in elements.get(subject, []):
                if start is not None and end is not None and start <= coll_date <= end:
                    # A day shared by two elements belongs to the later one.
                    if chosen is None or start >= chosen[1]:
                        chosen = (name, start)
            if chosen is not None:
                epoch = chosen[0]

        vsdy = ""
        ref = ref_start.get(subject)
        if coll_date is not None and ref is not None:
            diff = (coll_date - ref).days
            vsdy = str(diff + 1) if diff >= 0 else str(diff)

        out_rows.append({
            "DOMAIN": "VS",
            "STUDYID": row["STUDYID"],
            "USUBJID": subject,
            "VSSEQ": row["VSSEQ"],
            "VSTESTCD": row["VSTESTCD"],
            "VSORRES": row["VSORRES"],
            "VSDTC": row["VSDTC"],
            "VISIT": row["VISIT"],
            "VISITNUM": format_number(visitnum),
            "EPOCH": epoch,
            "VSDY": vsdy,
        })

    columns = ["DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VSTESTCD", "VSORRES",
               "VSDTC", "VISIT", "VISITNUM", "EPOCH", "VSDY"]
    with open(f"{OUTPUT_DIR}/vs.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
