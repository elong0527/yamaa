# Reference solution for the yamaa benchmark adam-adae-review-order (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

SEVERITY_RANK = {"SEVERE": 0, "MODERATE": 1, "MILD": 2}


def main():
    with open("/app/input/ae.csv", newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))

    subject_order = {}
    for row in rows:
        if row["USUBJID"] not in subject_order:
            subject_order[row["USUBJID"]] = len(subject_order)

    def review_key(row):
        severity = (row["AESEV"] or "").strip()
        rank = SEVERITY_RANK.get(severity, 3)
        onset = (row["AESTDTC"] or "").strip()
        # Within one severity, an event with no onset date comes first.
        return (
            subject_order[row["USUBJID"]],
            rank,
            0 if onset == "" else 1,
            onset,
            int(row["AESEQ"]),
        )

    rows.sort(key=review_key)

    with open("/app/output/adae.csv", "w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f, lineterminator="\n")
        writer.writerow(["STUDYID", "USUBJID", "ASEQ", "AETERM", "ASTDT", "ASEV"])
        for row in rows:
            writer.writerow(
                [
                    row["STUDYID"],
                    row["USUBJID"],
                    row["AESEQ"],
                    row["AETERM"],
                    row["AESTDTC"],
                    row["AESEV"],
                ]
            )


if __name__ == "__main__":
    main()
