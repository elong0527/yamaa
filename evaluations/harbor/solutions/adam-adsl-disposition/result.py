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
    """Last record: latest DSSTDTC, ties broken by higher DSSEQ."""
    if not records:
        return None
    return max(records, key=lambda r: (r["DSSTDTC"], float(r["DSSEQ"])))


def main():
    dm = read_csv(INPUT_DIR + "/dm.csv")
    ds = read_csv(INPUT_DIR + "/ds.csv")

    # Only DISPOSITION EVENT records with a date ever count.
    counted = {}
    for row in ds:
        if row["DSCAT"] != "DISPOSITION EVENT":
            continue
        if not row["DSSTDTC"]:
            continue
        counted.setdefault(row["USUBJID"], []).append(row)

    fieldnames = [
        "STUDYID", "USUBJID", "EOSDT", "EOSDECOD", "EOSREAS", "EOSSTT",
        "DCSREAS", "EOTDT", "EOTSTT", "DCTREAS", "DCSREASP",
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
            eosstt = "COMPLETED" if eosdecod == "COMPLETED" else "DISCONTINUED"
        dcsreas = eosreas if eosstt == "DISCONTINUED" else ""

        # End-of-treatment summary.
        if last_treat is None:
            eotdt = ""
            eotstt = "ONGOING"
            treat_term = ""
            treat_decod = ""
        else:
            eotdt = last_treat["DSSTDTC"]
            treat_decod = last_treat["DSDECOD"]
            treat_term = last_treat["DSTERM"]
            eotstt = "COMPLETED" if treat_decod == "COMPLETED" else "DISCONTINUED"
        dctreas = treat_term if eotstt == "DISCONTINUED" else ""
        dcsreasp = treat_term if treat_decod == "OTHER" else ""

        out_rows.append({
            "STUDYID": subj["STUDYID"],
            "USUBJID": usubjid,
            "EOSDT": eosdt,
            "EOSDECOD": eosdecod,
            "EOSREAS": eosreas,
            "EOSSTT": eosstt,
            "DCSREAS": dcsreas,
            "EOTDT": eotdt,
            "EOTSTT": eotstt,
            "DCTREAS": dctreas,
            "DCSREASP": dcsreasp,
        })

    with open(OUTPUT_FILE, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(out_rows)


if __name__ == "__main__":
    main()
