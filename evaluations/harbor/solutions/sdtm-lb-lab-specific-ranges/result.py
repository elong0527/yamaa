# Reference solution for the yamaa benchmark sdtm-lb-lab-specific-ranges (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM LB dataset from the ODM extract and the LBRANGE table.

One record per collected lab result (one IT.LB.LBDTC item of a lab panel).
The reference range for a result is the LBRANGE entry for the result's lab,
test code and sex whose age band holds the subject's age in whole years at
collection and which was in effect on the collection date.
"""

import csv
from datetime import date

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def parse_date(text):
    parts = text.split("-")
    return date(int(parts[0]), int(parts[1]), int(parts[2]))


def age_in_years(birth, on):
    years = on.year - birth.year
    if (on.month, on.day) < (birth.month, birth.day):
        years -= 1
    return years


def read_odm(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def read_ranges(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def main():
    odm_rows = read_odm(INPUT_DIR + "/odm.csv")
    range_rows = read_ranges(INPUT_DIR + "/lbrange.csv")

    study_id = odm_rows[0]["StudyOID"]

    # Per-subject demographics and per-panel lab items, in collected order.
    subjects = {}
    panels = []
    panel_by_key = {}
    for row in odm_rows:
        subject = row["SubjectKey"]
        if row["ItemGroupOID"] == "IG.DM":
            demo = subjects.setdefault(subject, {})
            if row["ItemOID"] == "IT.DM.SEX":
                demo["SEX"] = row["Value"]
            elif row["ItemOID"] == "IT.DM.BRTHDTC":
                demo["BRTHDTC"] = row["Value"]
        elif row["ItemGroupOID"] == "IG.LB":
            key = (
                subject,
                row["StudyEventOID"],
                row["StudyEventRepeatKey"],
                row["ItemGroupOID"],
                row["ItemGroupRepeatKey"],
            )
            panel = panel_by_key.get(key)
            if panel is None:
                panel = {"subject": subject, "repeat": row["ItemGroupRepeatKey"], "items": {}}
                panel_by_key[key] = panel
                panels.append(panel)
            panel["items"][row["ItemOID"]] = row["Value"]

    out_rows = []
    for panel in panels:
        items = panel["items"]
        if "IT.LB.LBDTC" not in items:
            continue
        subject = panel["subject"]
        demo = subjects.get(subject, {})
        sex = demo.get("SEX", "")
        testcd = items.get("IT.LB.LBTESTCD", "")
        lab = items.get("IT.LB.LBNAM", "")
        result_text = items.get("IT.LB.LBSTRESN", "")
        collection = parse_date(items["IT.LB.LBDTC"])
        age = age_in_years(parse_date(demo["BRTHDTC"]), collection)

        candidates = []
        for entry in range_rows:
            if entry["LBNAM"] != lab or entry["LBTESTCD"] != testcd or entry["SEX"] != sex:
                continue
            if not (int(entry["AGELO"]) <= age <= int(entry["AGEHI"])):
                continue
            if parse_date(entry["EFFSTDT"]) > collection:
                continue
            if entry["EFFENDT"] and collection > parse_date(entry["EFFENDT"]):
                continue
            candidates.append(entry)
        match = None
        if candidates:
            # An age on a band boundary takes the band that starts at it.
            match = next(
                (entry for entry in candidates if int(entry["AGELO"]) == age),
                candidates[0],
            )

        if match is not None:
            unit = match["UNIT"]
            nrlo_text = match["NRLO"]
            nrhi_text = match["NRHI"]
        else:
            unit = ""
            nrlo_text = ""
            nrhi_text = ""

        nrind = ""
        if result_text and nrlo_text and nrhi_text:
            value = float(result_text)
            if value < float(nrlo_text):
                nrind = "LOW"
            elif value > float(nrhi_text):
                nrind = "HIGH"
            else:
                nrind = "NORMAL"

        out_rows.append(
            {
                "DOMAIN": "LB",
                "STUDYID": study_id,
                "USUBJID": subject,
                "LBSEQ": panel["repeat"],
                "LBTESTCD": testcd,
                "SEX": sex,
                "LBNAM": lab,
                "LBSTRESN": result_text,
                "LBORRESU": unit,
                "LBSTNRLO": nrlo_text,
                "LBSTNRHI": nrhi_text,
                "LBNRIND": nrind,
            }
        )

    columns = [
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "LBSEQ",
        "LBTESTCD",
        "SEX",
        "LBNAM",
        "LBSTRESN",
        "LBORRESU",
        "LBSTNRLO",
        "LBSTNRHI",
        "LBNRIND",
    ]
    with open(OUTPUT_DIR + "/lb.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
