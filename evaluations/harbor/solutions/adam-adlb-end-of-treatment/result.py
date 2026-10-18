# Reference solution for the yamaa benchmark adam-adlb-end-of-treatment (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = ["STUDYID", "USUBJID", "LBSEQ", "LBTESTCD", "VISITNUM",
           "AVAL", "ENDPOINT", "EOTFL"]


def read_csv(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def main():
    lb = read_csv(INPUT_DIR + "/lb.csv")
    supplb = read_csv(INPUT_DIR + "/supplb.csv")

    # Endpoint-marked keys: (STUDYID, USUBJID, LBSEQ as int). In SUPPLB the
    # sequence number is eight-character, space-padded text.
    endpoint_keys = set()
    for row in supplb:
        if row["QNAM"] == "ENDPOINT":
            endpoint_keys.add(
                (row["STUDYID"], row["USUBJID"], int(row["IDVARVAL"].strip()))
            )

    records = []
    for row in lb:
        key = (row["STUDYID"], row["USUBJID"], int(row["LBSEQ"]))
        records.append({
            "STUDYID": row["STUDYID"],
            "USUBJID": row["USUBJID"],
            "LBSEQ": row["LBSEQ"],
            "LBTESTCD": row["LBTESTCD"],
            "VISITNUM": row["VISITNUM"],
            "AVAL": row["LBSTRESN"],
            "ENDPOINT": "Y" if key in endpoint_keys else "",
            "EOTFL": "",
            "_visit": int(row["VISITNUM"]),
        })

    # One EOTFL "Y" per subject and test: the endpoint-marked record when
    # there is one, otherwise the record from the latest visit.
    groups = {}
    for i, rec in enumerate(records):
        groups.setdefault((rec["USUBJID"], rec["LBTESTCD"]), []).append(i)
    for idxs in groups.values():
        marked = [i for i in idxs if records[i]["ENDPOINT"] == "Y"]
        if marked:
            winner = marked[0]
        else:
            winner = max(idxs, key=lambda i: records[i]["_visit"])
        records[winner]["EOTFL"] = "Y"

    with open(OUTPUT_DIR + "/adlb.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=COLUMNS)
        writer.writeheader()
        for rec in records:
            writer.writerow({c: rec[c] for c in COLUMNS})


if __name__ == "__main__":
    main()
