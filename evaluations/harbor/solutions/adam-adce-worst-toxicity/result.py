# Reference solution for the yamaa benchmark adam-adce-worst-toxicity (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADCE from CE: one record per clinical event, with severity grade
and a flag on each subject's worst-toxicity event."""

import csv
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

SEVERITY_GRADE = {"MILD": 1, "MODERATE": 2, "SEVERE": 3}


def main() -> None:
    with (INPUT / "ce.csv").open(newline="") as f:
        ce = list(csv.DictReader(f))

    rows = []
    for rec in ce:
        asev = rec["CESEV"] or ""
        grade = SEVERITY_GRADE.get(asev) if asev else None
        rows.append(
            {
                "STUDYID": rec["STUDYID"],
                "USUBJID": rec["USUBJID"],
                "CESEQ": rec["CESEQ"],
                "CETERM": rec["CETERM"],
                "ASTDT": rec["ASTDT"],
                "ASEV": asev,
                "ASEVN": grade,
                "ATOXGRN": grade,
                "AOCCFL": "",
            }
        )

    # Flag, per subject, the event with the greatest grade; ties break to
    # the earliest ASTDT and then the lowest CESEQ. Ungraded events are
    # never flagged.
    by_subject: dict[str, list[dict]] = {}
    for row in rows:
        by_subject.setdefault(row["USUBJID"], []).append(row)
    for subject_rows in by_subject.values():
        graded = [r for r in subject_rows if r["ATOXGRN"] is not None]
        if not graded:
            continue
        best = min(
            graded,
            key=lambda r: (-r["ATOXGRN"], r["ASTDT"], int(r["CESEQ"])),
        )
        best["AOCCFL"] = "Y"

    columns = [
        "STUDYID", "USUBJID", "CESEQ", "CETERM", "ASTDT",
        "ASEV", "ASEVN", "ATOXGRN", "AOCCFL",
    ]
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with (OUTPUT / "adce.csv").open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        for row in rows:
            writer.writerow(
                {c: ("" if row[c] is None else row[c]) for c in columns}
            )


if __name__ == "__main__":
    main()
