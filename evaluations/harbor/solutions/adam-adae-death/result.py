# Reference solution for the yamaa benchmark adam-adae-death (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Create an ADAE dataset (one record per adverse event) from AE and DM,
deriving the death flag, death cause, and death date per subject.

Rules:
- A fatal event is an event whose outcome (AEOUT) is "FATAL".
- DTHFL is "Y" on every event of a subject who has a fatal event or a
  death date in DM; it has no value on every other event.
- DTHCAUS is the coded term (AEDECOD) of the subject's fatal event;
  it has no value when no event of the subject was reported fatal.
- DTHDT is the start date of the subject's fatal event. When the subject
  has no fatal event, DTHDT is the death date recorded in DM. It has no
  value when the subject has neither a fatal event nor a DM death date.
- When a subject has more than one fatal event, the fatal event with
  the latest start date supplies DTHCAUS and DTHDT; a tie on start date
  goes to the fatal event with the highest AESEQ.
"""

import csv

INPUT_DIR = "/app/input"
OUTPUT_FILE = "/app/output/adae.csv"


def read_rows(path):
    with open(path, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


ae_rows = read_rows(f"{INPUT_DIR}/ae.csv")
dm_rows = read_rows(f"{INPUT_DIR}/dm.csv")

# Death date recorded in DM, per subject (only non-empty values count).
dm_death = {}
for row in dm_rows:
    if row["DTHDT"] != "":
        dm_death[row["USUBJID"]] = row["DTHDT"]

# Chosen fatal event per subject: latest ASTDT, tie broken by highest AESEQ.
chosen_fatal = {}
for row in ae_rows:
    if row["AEOUT"] != "FATAL":
        continue
    subject = row["USUBJID"]
    current = chosen_fatal.get(subject)
    if current is None or (row["ASTDT"], int(row["AESEQ"])) > (
        current["ASTDT"],
        int(current["AESEQ"]),
    ):
        chosen_fatal[subject] = row


def death_flag(subject):
    return "Y" if subject in chosen_fatal or subject in dm_death else ""


def death_cause(subject):
    return chosen_fatal[subject]["AEDECOD"] if subject in chosen_fatal else ""


def death_date(subject):
    if subject in chosen_fatal:
        return chosen_fatal[subject]["ASTDT"]
    return dm_death.get(subject, "")


fieldnames = ["STUDYID", "USUBJID", "AESEQ", "AEDECOD", "ASTDT", "DTHFL", "DTHCAUS", "DTHDT"]
with open(OUTPUT_FILE, "w", newline="", encoding="utf-8") as handle:
    writer = csv.DictWriter(handle, fieldnames=fieldnames)
    writer.writeheader()
    for row in ae_rows:
        subject = row["USUBJID"]
        writer.writerow(
            {
                "STUDYID": row["STUDYID"],
                "USUBJID": subject,
                "AESEQ": row["AESEQ"],
                "AEDECOD": row["AEDECOD"],
                "ASTDT": row["ASTDT"],
                "DTHFL": death_flag(subject),
                "DTHCAUS": death_cause(subject),
                "DTHDT": death_date(subject),
            }
        )
