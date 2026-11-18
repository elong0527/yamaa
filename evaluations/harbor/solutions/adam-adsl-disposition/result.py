# Reference solution for the yamaa benchmark adam-adsl-disposition (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"
OUTPUT_FILE = OUTPUT_DIR + "/adsl.csv"


def read_csv(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def last_record(records):
    """Last record: latest DSSTDTC, ties broken by higher DSSEQ; an undated
    record is last only when no record has a DSSTDTC."""
    if not records:
        return None
    return max(
        records,
        key=lambda r: (r["DSSTDTC"] != "", r["DSSTDTC"], float(r["DSSEQ"])),
    )


def main():
    dm = read_csv(INPUT_DIR + "/dm.csv")
    ds = read_csv(INPUT_DIR + "/ds.csv")

    # Only DISPOSITION EVENT records ever count.
    counted = {}
    for row in ds:
        if row["DSCAT"] != "DISPOSITION EVENT":
            continue
        counted.setdefault(row["USUBJID"], []).append(row)

    fieldnames = [
        "STUDYID", "USUBJID", "EOSDT", "EOSREAS", "EOSSTT",
        "DCSREAS", "EOTDT", "EOTSTT", "DCTREAS", "DCTREASP",
    ]

    out_rows = []
    for subj in dm:
        usubjid = subj["USUBJID"]
        records = counted.get(usubjid, [])
        study = [r for r in records if r["EPOCH"] != "TREATMENT"]
        treat = [r for r in records if r["EPOCH"] == "TREATMENT"]

        last_study = last_record(study)
        last_treat = last_record(treat)

        # End-of-study summary.
        if last_study is None:
            eosdt = eosdecod = eosreas = ""
            eosstt = "ONGOING"
        else:
            eosdt = last_study["DSSTDTC"]
            eosdecod = last_study["DSDECOD"]
            eosreas = last_study["DSTERM"]
            if eosdecod == "SCREEN FAILURE":
                # A screen failure never entered the study: no status.
                eosstt = ""
            elif eosdecod == "COMPLETED":
                eosstt = "COMPLETED"
            else:
                eosstt = "DISCONTINUED"
        dcsreas = eosdecod if eosstt == "DISCONTINUED" else ""

        # End-of-treatment summary; none for a subject never treated.
        if not subj["RFXSTDTC"]:
            eotdt = eotstt = treat_term = treat_decod = ""
            if last_treat is not None:
                eotdt = last_treat["DSSTDTC"]
        elif last_treat is None:
            eotdt = ""
            eotstt = "ONGOING"
            treat_term = ""
            treat_decod = ""
        else:
            eotdt = last_treat["DSSTDTC"]
            treat_decod = last_treat["DSDECOD"]
            treat_term = last_treat["DSTERM"]
            eotstt = "COMPLETED" if treat_decod == "COMPLETED" else "DISCONTINUED"
        dctreas = treat_decod if eotstt == "DISCONTINUED" else ""
        dctreasp = treat_term if dctreas == "OTHER" else ""

        out_rows.append({
            "STUDYID": subj["STUDYID"],
            "USUBJID": usubjid,
            "EOSDT": eosdt,
            "EOSREAS": eosreas,
            "EOSSTT": eosstt,
            "DCSREAS": dcsreas,
            "EOTDT": eotdt,
            "EOTSTT": eotstt,
            "DCTREAS": dctreas,
            "DCTREASP": dctreasp,
        })

    with open(OUTPUT_FILE, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
