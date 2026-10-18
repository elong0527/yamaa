# Reference solution for the yamaa benchmark adam-adae-exposure-adjusted-incidence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from datetime import date, timedelta
from pathlib import Path


INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")
TERMS = ["DIARRHOEA", "HEADACHE"]


def parse_date(value: str) -> date:
    return date.fromisoformat(value)


def main() -> None:
    with open(INPUT_DIR / "adsl.csv", newline="", encoding="utf-8") as handle:
        adsl_rows = list(csv.DictReader(handle))
    with open(INPUT_DIR / "adae.csv", newline="", encoding="utf-8") as handle:
        adae_rows = list(csv.DictReader(handle))

    # One subject record, retaining the order in which subjects appear.
    subjects = {}
    subject_order = []
    for row in adsl_rows:
        key = (row["STUDYID"], row["USUBJID"])
        first_dose = parse_date(row["TRTSDT"])
        last_dose = parse_date(row["TRTEDT"])
        if key not in subjects:
            subjects[key] = {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "TRT01A": row["TRT01A"],
                "first_dose": first_dose,
                "last_dose": last_dose,
            }
            subject_order.append(key)
        else:
            subjects[key]["first_dose"] = min(
                subjects[key]["first_dose"], first_dose
            )
            subjects[key]["last_dose"] = max(subjects[key]["last_dose"], last_dose)

    # Index event onset dates by subject and prespecified term.
    onsets = {}
    for row in adae_rows:
        if row["AEDECOD"] not in TERMS or not row["ASTDT"]:
            continue
        key = (row["STUDYID"], row["USUBJID"], row["AEDECOD"])
        onsets.setdefault(key, []).append(parse_date(row["ASTDT"]))

    risk_rows = []
    for key in subject_order:
        subject = subjects[key]
        first_dose = subject["first_dose"]
        window_end = subject["last_dose"] + timedelta(days=30)

        for term in TERMS:
            in_window = [
                onset
                for onset in onsets.get(
                    (subject["STUDYID"], subject["USUBJID"], term), []
                )
                if first_dose <= onset <= window_end
            ]
            first_date = min(in_window) if in_window else None
            event_n = 1 if in_window else 0
            risk_end = first_date if event_n else window_end

            risk_rows.append(
                {
                    "STUDYID": subject["STUDYID"],
                    "USUBJID": subject["USUBJID"],
                    "TRT01A": subject["TRT01A"],
                    "AEDECOD": term,
                    "FIRSTDT": first_date.isoformat() if first_date else "",
                    "EVENTN": event_n,
                    "RISKDAYS": (risk_end - first_dose).days + 1,
                }
            )

    # Aggregate in the order treatments and terms first appear in ADRISK.
    incidence = {}
    incidence_order = []
    for row in risk_rows:
        key = (row["STUDYID"], row["TRT01A"], row["AEDECOD"])
        if key not in incidence:
            incidence[key] = {
                "STUDYID": row["STUDYID"],
                "TRT01A": row["TRT01A"],
                "AEDECOD": row["AEDECOD"],
                "NSUBJ": 0,
                "PERSONDAYS": 0,
            }
            incidence_order.append(key)
        incidence[key]["NSUBJ"] += row["EVENTN"]
        incidence[key]["PERSONDAYS"] += row["RISKDAYS"]

    incidence_rows = []
    for key in incidence_order:
        row = incidence[key]
        person_years = row["PERSONDAYS"] / 365.25
        incidence_rows.append(
            {
                "STUDYID": row["STUDYID"],
                "TRT01A": row["TRT01A"],
                "AEDECOD": row["AEDECOD"],
                "NSUBJ": row["NSUBJ"],
                "PERSONDAYS": row["PERSONDAYS"],
                "PERSONYRS": person_years,
                "EAIR100": 100 * row["NSUBJ"] / person_years,
            }
        )

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    risk_fields = [
        "STUDYID",
        "USUBJID",
        "TRT01A",
        "AEDECOD",
        "FIRSTDT",
        "EVENTN",
        "RISKDAYS",
    ]
    with open(
        OUTPUT_DIR / "adrisk.csv", "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=risk_fields)
        writer.writeheader()
        writer.writerows(risk_rows)

    incidence_fields = [
        "STUDYID",
        "TRT01A",
        "AEDECOD",
        "NSUBJ",
        "PERSONDAYS",
        "PERSONYRS",
        "EAIR100",
    ]
    with open(
        OUTPUT_DIR / "adinc.csv", "w", newline="", encoding="utf-8"
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=incidence_fields)
        writer.writeheader()
        writer.writerows(incidence_rows)


if __name__ == "__main__":
    main()
