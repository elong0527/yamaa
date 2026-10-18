# Reference solution for the yamaa benchmark adam-advs-windows (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_PATH = "/app/output/advs.csv"

COLUMNS = [
    "STUDYID", "USUBJID", "PARAMCD", "VSSEQ", "VISIT", "VISITNUM",
    "ADT", "ADY", "AVAL", "AVISIT", "AVISITN", "ANL01FL",
]

AVISITN = {
    "SCREENING": -1,
    "BASELINE": 0,
    "WEEK 2": 2,
    "WEEK 4": 4,
    "POST-TREATMENT": 99,
}

EXPECTED_VISITS = ["SCREENING", "BASELINE", "WEEK 2", "WEEK 4"]


def window_for(ady):
    """Analysis visit whose window holds the study day, or None."""
    if ady is None:
        return None
    if ady < 0:
        return "SCREENING"
    if ady == 1:
        return "BASELINE"
    if 2 <= ady <= 21:
        return "WEEK 2"
    if 22 <= ady <= 42:
        return "WEEK 4"
    if ady >= 43:
        return "POST-TREATMENT"
    return None


def to_int(text):
    text = (text or "").strip()
    if text == "":
        return None
    return int(float(text))


def to_num_text(text):
    text = (text or "").strip()
    if text == "":
        return ""
    value = float(text)
    if value == int(value):
        return str(int(value))
    return repr(value)


def read_csv(path):
    with open(path, newline="") as handle:
        return list(csv.DictReader(handle))


def main():
    sv_rows = read_csv(INPUT_DIR + "/sv.csv")
    vs_rows = read_csv(INPUT_DIR + "/vs.csv")

    # Planned visits per subject from SV (visit name -> visit number).
    planned = OrderedDict()
    for row in sv_rows:
        subject = row["USUBJID"]
        planned.setdefault(subject, OrderedDict())
        if row["VISIT"] in EXPECTED_VISITS:
            planned[subject][row["VISIT"]] = to_int(row["VISITNUM"])

    records = []
    subjects = OrderedDict()  # subject -> {"max_seq": int, "windows": set}
    for row in vs_rows:
        subject = row["USUBJID"]
        ady = to_int(row["VSDY"])
        avisit = window_for(ady)
        info = subjects.setdefault(subject, {"max_seq": 0, "windows": set()})
        seq = to_int(row["VSSEQ"])
        info["max_seq"] = max(info["max_seq"], seq)
        if avisit is not None:
            info["windows"].add(avisit)
        records.append(OrderedDict([
            ("STUDYID", row["STUDYID"]),
            ("USUBJID", subject),
            ("PARAMCD", row["VSTESTCD"]),
            ("VSSEQ", seq),
            ("VISIT", row["VISIT"]),
            ("VISITNUM", to_int(row["VISITNUM"])),
            ("ADT", (row["VSDTC"] or "").strip()),
            ("ADY", ady),
            ("AVAL", to_num_text(row["VSSTRESN"])),
            ("AVISIT", avisit or ""),
            ("AVISITN", AVISITN[avisit] if avisit else None),
            ("ANL01FL", ""),
            ("_expected", False),
        ]))

    # ANL01FL: earliest by study day, lower VSSEQ breaks a tie, per
    # study/subject/parameter/analysis visit. Records with no study day
    # (hence no window) never take the flag.
    best = {}
    for rec in records:
        if rec["ADY"] is None or rec["AVISIT"] == "":
            continue
        key = (rec["STUDYID"], rec["USUBJID"], rec["PARAMCD"], rec["AVISIT"])
        rank = (rec["ADY"], rec["VSSEQ"])
        if key not in best or rank < best[key][0]:
            best[key] = (rank, rec)
    for _, rec in best.values():
        rec["ANL01FL"] = "Y"

    # Expected records for planned visits whose window holds no measured
    # study day for the subject.
    expected_by_subject = OrderedDict()
    for subject, visits in planned.items():
        info = subjects.get(subject, {"max_seq": 0, "windows": set()})
        missing = [
            (num, name) for name, num in visits.items()
            if name not in info["windows"]
        ]
        missing.sort(key=lambda item: (item[0] is None, item[0]))
        expected = []
        for place, (num, name) in enumerate(missing, start=1):
            expected.append(OrderedDict([
                ("STUDYID", next(
                    r["STUDYID"] for r in sv_rows if r["USUBJID"] == subject)),
                ("USUBJID", subject),
                ("PARAMCD", "SYSBP"),
                ("VSSEQ", info["max_seq"] + place),
                ("VISIT", name),
                ("VISITNUM", num),
                ("ADT", ""),
                ("ADY", None),
                ("AVAL", ""),
                ("AVISIT", name),
                ("AVISITN", AVISITN[name]),
                ("ANL01FL", ""),
                ("_expected", True),
            ]))
        expected_by_subject[subject] = expected

    # Emit collected records in input order, with each subject's expected
    # records (VSSEQ continuing the subject's sequence) after them.
    last_index = {}
    for i, rec in enumerate(records):
        last_index[rec["USUBJID"]] = i
    output = []
    for i, rec in enumerate(records):
        output.append(rec)
        if last_index[rec["USUBJID"]] == i:
            output.extend(expected_by_subject.get(rec["USUBJID"], []))
    for subject, expected in expected_by_subject.items():
        if subject not in last_index:
            output.extend(expected)

    with open(OUTPUT_PATH, "w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(COLUMNS)
        for rec in output:
            writer.writerow([
                "" if rec[col] is None else rec[col] for col in COLUMNS
            ])


if __name__ == "__main__":
    main()
