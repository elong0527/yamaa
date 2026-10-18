# Reference solution for the yamaa benchmark adam-adice-governing-event (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/adice.csv"

PRIORITY = {
    "ICE04": 1,
    "ICE05": 1,
    "ICE01": 2,
    "ICE02": 3,
    "ICE03": 4,
}

COLUMNS = [
    "STUDYID",
    "USUBJID",
    "ASEQ",
    "ADECOD1",
    "ASTDT",
    "ICEPRIO",
    "ICERANK",
    "GOVFL",
    "SUPERFL",
    "GOVTYPE",
    "GOVDT",
    "GOVSEQ",
]


def main():
    with open(f"{INPUT_DIR}/adsl.csv", newline="") as f:
        adsl = list(csv.DictReader(f))
    subjects = {(r["STUDYID"], r["USUBJID"]) for r in adsl}

    with open(f"{INPUT_DIR}/ice_raw.csv", newline="") as f:
        ice = list(csv.DictReader(f))

    events = {}
    order = []
    for row in ice:
        key = (row["STUDYID"], row["USUBJID"])
        if key not in subjects:
            continue
        event = {
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "ASEQ": int(row["ICESEQ"]),
            "ADECOD1": row["ICETYPE"],
            "ASTDT": row["ICEDTC"],
            "ICEPRIO": PRIORITY[row["ICETYPE"]],
        }
        events.setdefault(key, []).append(event)
        order.append(event)

    for key, subject_events in events.items():
        ranked = sorted(
            subject_events,
            key=lambda e: (e["ICEPRIO"], e["ASTDT"], e["ASEQ"]),
        )
        governing = ranked[0]
        for position, event in enumerate(ranked, start=1):
            event["ICERANK"] = position
            event["GOVFL"] = "Y" if position == 1 else ""
            event["SUPERFL"] = "" if position == 1 else "Y"
            event["GOVTYPE"] = governing["ADECOD1"]
            event["GOVDT"] = governing["ASTDT"]
            event["GOVSEQ"] = governing["ASEQ"]

    with open(OUTPUT_PATH, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        for event in order:
            writer.writerow({c: event[c] for c in COLUMNS})


if __name__ == "__main__":
    main()
