# Reference solution for the yamaa benchmark adam-adlb-bds (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def fmt(value):
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    if value == int(value) and abs(value) < 1e15:
        return str(int(value))
    return repr(value)


def main():
    adsl = {}
    with open(f"{INPUT_DIR}/adsl.csv", newline="") as f:
        for row in csv.DictReader(f):
            adsl[row["USUBJID"]] = row

    params = {
        "ALT": ("ALT", "Alanine Aminotransferase"),
        "AST": ("AST", "Aspartate Aminotransferase"),
    }

    records = []
    with open(f"{INPUT_DIR}/lb.csv", newline="") as f:
        for row in csv.DictReader(f):
            testcd = row["LBTESTCD"]
            if testcd not in params:
                continue
            raw = (row["LBSTRESN"] or "").strip()
            if raw == "":
                continue
            try:
                aval = float(raw)
            except ValueError:
                continue
            subj = adsl.get(row["USUBJID"], {})
            base_rec = {
                "STUDYID": row["STUDYID"],
                "USUBJID": row["USUBJID"],
                "ADT": row["LBDTC"],
                "TRTSDT": subj.get("TRTSDT", ""),
                "TRT01A": subj.get("TRT01A", ""),
            }
            paramcd, param = params[testcd]
            rec = dict(base_rec)
            rec.update({
                "PARAMCD": paramcd,
                "PARAM": param,
                "AVAL": aval,
                "AVAL_TEXT": raw,
                "AVALU": row["LBSTRESU"],
            })
            records.append(rec)
            if testcd == "ALT":
                si = dict(base_rec)
                si.update({
                    "PARAMCD": "ALTSI",
                    "PARAM": "Alanine Aminotransferase (SI)",
                    "AVAL": aval * 0.0167,
                    "AVAL_TEXT": None,
                    "AVALU": "ukat/L",
                })
                records.append(si)

    # Baseline: latest record on or before TRTSDT per subject + parameter.
    groups = OrderedDict()
    for rec in records:
        groups.setdefault((rec["USUBJID"], rec["PARAMCD"]), []).append(rec)

    for key, grecs in groups.items():
        baseline = None
        for rec in grecs:
            if rec["TRTSDT"] and rec["ADT"] <= rec["TRTSDT"]:
                if baseline is None or rec["ADT"] > baseline["ADT"]:
                    baseline = rec
        for rec in grecs:
            rec["ABLFL"] = "Y" if rec is baseline else ""
            if baseline is None:
                rec["BASE"] = None
                rec["CHG"] = None
                rec["PCHG"] = None
            else:
                base = baseline["AVAL"]
                chg = rec["AVAL"] - base
                rec["BASE"] = base
                rec["CHG"] = chg
                rec["PCHG"] = None if base == 0 else 100.0 * chg / base

    records.sort(key=lambda r: (r["USUBJID"], r["PARAMCD"], r["ADT"]))

    by_subject = OrderedDict()
    for rec in records:
        by_subject.setdefault(rec["USUBJID"], []).append(rec)
    for subj, srecs in by_subject.items():
        for i, rec in enumerate(srecs, start=1):
            rec["ASEQ"] = i

    columns = ["STUDYID", "USUBJID", "PARAMCD", "PARAM", "ADT", "TRTSDT",
               "TRT01A", "AVAL", "AVALU", "ABLFL", "BASE", "CHG", "PCHG",
               "ASEQ"]
    with open(f"{OUTPUT_DIR}/adlb.csv", "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(columns)
        for rec in records:
            writer.writerow([
                rec["STUDYID"],
                rec["USUBJID"],
                rec["PARAMCD"],
                rec["PARAM"],
                rec["ADT"],
                rec["TRTSDT"],
                rec["TRT01A"],
                rec["AVAL_TEXT"] if rec["AVAL_TEXT"] is not None
                else fmt(rec["AVAL"]),
                rec["AVALU"],
                rec["ABLFL"],
                fmt(rec["BASE"]),
                fmt(rec["CHG"]),
                fmt(rec["PCHG"]),
                rec["ASEQ"],
            ])


if __name__ == "__main__":
    main()
