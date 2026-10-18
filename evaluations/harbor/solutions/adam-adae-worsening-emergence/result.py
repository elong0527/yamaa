# Reference solution for the yamaa benchmark adam-adae-worsening-emergence (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build ADAE with one record per subject per adverse event per analysis
period, carrying forward the most recently collected severity and toxicity
grade, and flagging treatment-emergent worsening after first exposure."""

import csv
from datetime import datetime

INPUT = "/app/input"
OUTPUT = "/app/output"

SEV_RANK = {"MILD": 1, "MODERATE": 2, "SEVERE": 3}


def read_csv(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def parse_dt(text):
    text = (text or "").strip()
    if not text:
        return None
    return datetime.fromisoformat(text)


def fmt_dt(value):
    if value is None:
        return ""
    return value.strftime("%Y-%m-%dT%H:%M:%S")


def main():
    adsl = read_csv(f"{INPUT}/adsl.csv")
    ae = read_csv(f"{INPUT}/ae.csv")
    fa = read_csv(f"{INPUT}/fa.csv")

    # First exposure per subject (blank for untreated subjects).
    trtsdtm = {}
    for row in adsl:
        trtsdtm[row["USUBJID"]] = row["TRTSDTM"]

    # FA findings per (subject, link id): lists of (time, value) per test.
    findings = {}
    for row in fa:
        key = (row["USUBJID"], row["FALNKID"])
        entry = findings.setdefault(key, {"SEV": [], "TOXGR": [], "times": set()})
        when = parse_dt(row["FADTC"])
        entry["times"].add(when)
        if row["FATESTCD"] == "SEV":
            entry["SEV"].append((when, row["FASTRESC"]))
        elif row["FATESTCD"] == "TOXGR":
            entry["TOXGR"].append((when, row["FASTRESC"]))
    for entry in findings.values():
        entry["SEV"].sort(key=lambda item: item[0])
        entry["TOXGR"].sort(key=lambda item: item[0])

    def most_recent(items, start):
        """Value most recently collected at or before start, else None."""
        value = None
        for when, val in items:
            if when <= start:
                value = val
            else:
                break
        return value

    out_rows = []
    # Events in AESEQ order within each subject, subjects as collected.
    events = sorted(
        ae,
        key=lambda r: (r["USUBJID"], int(r["AESEQ"])),
    )
    subjects = []
    for row in events:
        if row["USUBJID"] not in subjects:
            subjects.append(row["USUBJID"])
    events.sort(key=lambda r: (subjects.index(r["USUBJID"]), int(r["AESEQ"])))

    for subj in subjects:
        subj_events = [r for r in events if r["USUBJID"] == subj]
        exposure = parse_dt(trtsdtm.get(subj, ""))
        aseq = 0
        for ev in subj_events:
            onset = parse_dt(ev["AESTDTC"])
            entry = findings.get((subj, ev["AELNKID"]), {"SEV": [], "TOXGR": [], "times": set()})
            starts = sorted({onset} | entry["times"])

            periods = []
            for i, start in enumerate(starts):
                if i + 1 < len(starts):
                    end = fmt_dt(starts[i + 1])
                else:
                    # Last period ends at the event end; an ongoing event
                    # (no AEENDTC) has no AENDTM.
                    end = ev["AEENDTC"] if ev["AEENDTC"] else ""
                asev = most_recent(entry["SEV"], start)
                grade = most_recent(entry["TOXGR"], start)
                periods.append(
                    {
                        "start": start,
                        "ASTDTM": fmt_dt(start),
                        "AENDTM": end,
                        "ASEV": asev if asev else "",
                        "ATOXGRN": int(grade) if grade not in (None, "") else None,
                    }
                )

            # Treatment-emergent flag.
            flags = [""] * len(periods)
            if exposure is not None:
                if onset >= exposure:
                    flags = ["Y"] * len(periods)
                else:
                    # Reference: the last pre-treatment period's values.
                    pre = [p for p in periods if p["start"] < exposure]
                    ref_sev = SEV_RANK.get(pre[-1]["ASEV"]) if pre else None
                    ref_grade = pre[-1]["ATOXGRN"] if pre else None
                    emergent = False
                    for i, p in enumerate(periods):
                        if p["start"] < exposure:
                            continue
                        if not emergent:
                            sev_rank = SEV_RANK.get(p["ASEV"])
                            sev_up = (
                                sev_rank is not None
                                and ref_sev is not None
                                and sev_rank > ref_sev
                            )
                            grade_up = (
                                p["ATOXGRN"] is not None
                                and ref_grade is not None
                                and p["ATOXGRN"] > ref_grade
                            )
                            if sev_up or grade_up:
                                emergent = True
                        if emergent:
                            flags[i] = "Y"

            for p, flag in zip(periods, flags):
                aseq += 1
                out_rows.append(
                    {
                        "STUDYID": ev["STUDYID"],
                        "USUBJID": subj,
                        "ASEQ": aseq,
                        "AESEQ": ev["AESEQ"],
                        "AEDECOD": ev["AEDECOD"],
                        "AESTDTC": ev["AESTDTC"],
                        "AEENDTC": ev["AEENDTC"],
                        "ASTDTM": p["ASTDTM"],
                        "AENDTM": p["AENDTM"],
                        "TRTSDTM": trtsdtm.get(subj, ""),
                        "AESEV": ev["AESEV"],
                        "AETOXGR": ev["AETOXGR"],
                        "ASEV": p["ASEV"],
                        "ATOXGRN": "" if p["ATOXGRN"] is None else p["ATOXGRN"],
                        "TRTEMFL": flag,
                    }
                )

    columns = [
        "STUDYID", "USUBJID", "ASEQ", "AESEQ", "AEDECOD", "AESTDTC",
        "AEENDTC", "ASTDTM", "AENDTM", "TRTSDTM", "AESEV", "AETOXGR",
        "ASEV", "ATOXGRN", "TRTEMFL",
    ]
    with open(f"{OUTPUT}/adae.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=columns)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
