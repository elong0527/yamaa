# Reference solution for the yamaa benchmark adam-adaecm-related-pairs (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")
COLUMNS = ["STUDYID", "USUBJID", "AESEQ", "AETERM", "CMSEQ", "CMTRT", "RELID"]


def read(name: str) -> list[dict[str, str]]:
    with open(INPUT_DIR / name, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def main() -> None:
    ae = read("ae.csv")
    cm = read("cm.csv")
    relrec = read("relrec.csv")

    terms = {(r["STUDYID"], r["USUBJID"], int(r["AESEQ"])): r["AETERM"] for r in ae}
    names = {(r["STUDYID"], r["USUBJID"], int(r["CMSEQ"])): r["CMTRT"] for r in cm}
    events = [r for r in relrec if r["RDOMAIN"] == "AE" and r["IDVAR"] == "AESEQ"]
    medications = [
        r for r in relrec if r["RDOMAIN"] == "CM" and r["IDVAR"] == "CMSEQ"
    ]

    # Every event of a relationship with every medication of the same
    # subject's same relationship.
    rows = []
    for event in events:
        for medication in medications:
            if (event["STUDYID"], event["USUBJID"], event["RELID"]) != (
                medication["STUDYID"],
                medication["USUBJID"],
                medication["RELID"],
            ):
                continue
            aeseq = int(event["IDVARVAL"])
            cmseq = int(medication["IDVARVAL"])
            rows.append(
                {
                    "STUDYID": event["STUDYID"],
                    "USUBJID": event["USUBJID"],
                    "AESEQ": aeseq,
                    "AETERM": terms[(event["STUDYID"], event["USUBJID"], aeseq)],
                    "CMSEQ": cmseq,
                    "CMTRT": names[(event["STUDYID"], event["USUBJID"], cmseq)],
                    "RELID": event["RELID"],
                }
            )
    rows.sort(key=lambda r: (r["STUDYID"], r["USUBJID"], r["AESEQ"], r["CMSEQ"]))

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_DIR / "adaecm.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(COLUMNS)
        for row in rows:
            writer.writerow([row[column] for column in COLUMNS])


if __name__ == "__main__":
    main()
