# Reference solution for the yamaa benchmark sdtm-rs-timepoint-response (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import os
from collections import defaultdict

INPUT = "/app/input"
OUTPUT = "/app/output"

COLUMNS = [
    "STUDYID", "USUBJID", "AVISIT", "AVISITN", "ADT", "RSSEQ",
    "RSTESTCD", "RSTEST", "RSSTRESC", "RSSTAT",
]


def load_items(path):
    """Pivot the long ODM extract into per-group item dictionaries."""
    groups = {}
    order = []
    with open(path, newline="") as handle:
        for row in csv.DictReader(handle):
            key = (
                row["StudyOID"], row["SubjectKey"], row["StudyEventOID"],
                row["StudyEventRepeatKey"], row["ItemGroupOID"],
                row["ItemGroupRepeatKey"],
            )
            if key not in groups:
                groups[key] = {
                    "study": row["StudyOID"],
                    "subject": row["SubjectKey"],
                    "event": row["StudyEventOID"],
                    "group": row["ItemGroupOID"],
                    "items": {},
                }
                order.append(key)
            groups[key]["items"][row["ItemOID"]] = (row["Value"] or "").strip()
    return [groups[key] for key in order]


def to_number(text):
    if text in (None, ""):
        return None
    try:
        return float(text)
    except ValueError:
        return None


def main():
    records = load_items(os.path.join(INPUT, "odm.csv"))

    visits = defaultdict(list)          # subject -> [visit dicts]
    chosen_targets = defaultdict(set)   # subject -> {lesion ids chosen}
    target_meas = defaultdict(dict)     # (subject, event) -> {lesion: ldiam}
    any_meas = defaultdict(bool)       # (subject, event) -> any tumor record

    for rec in records:
        subject = rec["subject"]
        items = rec["items"]
        if rec["group"] == "IG.VISIT":
            visits[subject].append({
                "event": rec["event"],
                "avisit": items.get("IT.VISIT.AVISIT", ""),
                "avisitn": items.get("IT.VISIT.AVISITN", ""),
                "adt": items.get("IT.VISIT.ADT", ""),
            })
        elif rec["group"] == "IG.TUTARGET":
            link = items.get("IT.TU.TULNKID", "")
            if link:
                chosen_targets[subject].add(link)
        elif rec["group"] == "IG.TRTARGET":
            link = items.get("IT.TR.TRLNKID", "")
            if link:
                key = (subject, rec["event"])
                any_meas[key] = True
                target_meas[key][link] = to_number(items.get("IT.TR.LDIAM", ""))
        elif rec["group"] in ("IG.TRNT", "IG.TUNT"):
            any_meas[(subject, rec["event"])] = True

    rows = []
    study_of = {}
    for rec in records:
        study_of.setdefault(rec["subject"], rec["study"])
    for subject in sorted(visits):
        subject_visits = sorted(
            visits[subject],
            key=lambda v: (to_number(v["avisitn"]) is None,
                           to_number(v["avisitn"]) or 0),
        )
        if not subject_visits:
            continue
        baseline_event = subject_visits[0]["event"]
        chosen = chosen_targets.get(subject, set())
        baseline_sum = None
        if chosen:
            base = target_meas.get((subject, baseline_event), {})
            measured = [base[lesion] for lesion in chosen
                        if base.get(lesion) is not None]
            if len(measured) == len(chosen):
                baseline_sum = sum(measured)

        for seq, visit in enumerate(subject_visits, start=1):
            event = visit["event"]
            done = any_meas.get((subject, event), False)
            rsstresc = ""
            rsstat = ""
            if not done:
                rsstat = "NOT DONE"
            elif event == baseline_event:
                rsstresc = "NE"
            elif not chosen:
                # Non-target disease only.
                rsstresc = "NE"
            else:
                meas = target_meas.get((subject, event), {})
                measured = {lesion: meas[lesion] for lesion in chosen
                            if meas.get(lesion) is not None}
                if len(measured) < len(chosen):
                    rsstresc = "NE"
                elif baseline_sum is None or baseline_sum == 0:
                    rsstresc = "NE"
                else:
                    total = sum(measured.values())
                    if all(value == 0 for value in measured.values()):
                        rsstresc = "CR"
                    else:
                        change = 100.0 * (total - baseline_sum) / baseline_sum
                        if change <= -30.0:
                            rsstresc = "PR"
                        elif change >= 20.0:
                            rsstresc = "PD"
                        else:
                            rsstresc = "SD"
            rows.append({
                "STUDYID": study_of.get(subject, ""),
                "USUBJID": subject,
                "AVISIT": visit["avisit"],
                "AVISITN": visit["avisitn"],
                "ADT": visit["adt"],
                "RSSEQ": seq,
                "RSTESTCD": "TRGRESP",
                "RSTEST": "Timepoint Response",
                "RSSTRESC": rsstresc,
                "RSSTAT": rsstat,
            })

    os.makedirs(OUTPUT, exist_ok=True)
    with open(os.path.join(OUTPUT, "rs.csv"), "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
