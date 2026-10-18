# Reference solution for the yamaa benchmark adam-adae-severity-rank (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the ADAE severity-rank dataset from the AE dataset.

One record per adverse event. ASEVN is the numeric form of the
reported severity (MILD=1, MODERATE=2, SEVERE=3, blank when no
severity was reported). SEVRANK is the competition rank of the event
within its subject, worst severity first: events sharing a severity
share a rank and the ranks they would otherwise fill are skipped, and
events with no reported severity rank after every reported event and
share one rank. SEVLVL is the dense rank: each distinct severity,
worst first, takes the next number, with an unreported severity
counting as one further distinct severity.
"""

import csv
import os
from collections import defaultdict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

SEVERITY_CODES = {"MILD": 1, "MODERATE": 2, "SEVERE": 3}

COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "ASEV", "ASEVN", "SEVRANK", "SEVLVL"]


def main():
    with open(os.path.join(INPUT_DIR, "ae.csv"), newline="") as handle:
        events = list(csv.DictReader(handle))

    by_subject = defaultdict(list)
    for event in events:
        by_subject[event["USUBJID"]].append(event)

    rank_of = {}  # id(event) -> (SEVRANK, SEVLVL)
    for subject_events in by_subject.values():
        for event in subject_events:
            event["_asevn"] = SEVERITY_CODES.get(event["AESEV"].strip())

        reported = [e["_asevn"] for e in subject_events if e["_asevn"] is not None]
        distinct = sorted(set(reported), reverse=True)
        dense = {value: i + 1 for i, value in enumerate(distinct)}
        n_reported = len(reported)

        for event in subject_events:
            asevn = event["_asevn"]
            if asevn is None:
                # After every event with a reported severity; all
                # unreported events share one rank / one level. With
                # no reported events at all this yields rank 1, level 1.
                sevrank = n_reported + 1
                sevlvl = len(distinct) + 1
            else:
                sevrank = 1 + sum(1 for value in reported if value > asevn)
                sevlvl = dense[asevn]
            rank_of[id(event)] = (sevrank, sevlvl)

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    with open(os.path.join(OUTPUT_DIR, "adae.csv"), "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        for event in events:
            sevrank, sevlvl = rank_of[id(event)]
            asevn = event["_asevn"]
            writer.writerow(
                {
                    "STUDYID": event["STUDYID"],
                    "USUBJID": event["USUBJID"],
                    "AESEQ": event["AESEQ"],
                    "ASEV": event["AESEV"],
                    "ASEVN": "" if asevn is None else asevn,
                    "SEVRANK": sevrank,
                    "SEVLVL": sevlvl,
                }
            )


if __name__ == "__main__":
    main()
