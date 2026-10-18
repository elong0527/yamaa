# Reference solution for the yamaa benchmark sdtm-relrec-links (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build a RELREC dataset from AE and CM link identifiers.

Each AE or CM record contributes one RELREC row per distinct link
identifier it carries (AELNKID1/AELNKID2 for AE, CMLNKID1/CMLNKID2 for
CM). Records with no link identifier contribute no row. RELTYPE is
blank on every row because each row points at a single record.
"""

import csv
import os

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"

COLUMNS = ["STUDYID", "USUBJID", "RDOMAIN", "IDVAR", "IDVARVAL",
           "RELTYPE", "RELID"]


def rows_for(domain, seqvar, linkvars, path):
    """Yield one RELREC row per record/participating link identifier."""
    with open(path, newline="", encoding="utf-8") as handle:
        for record in csv.DictReader(handle):
            seen = []
            for var in linkvars:
                link = (record.get(var) or "").strip()
                if link and link not in seen:
                    seen.append(link)
            for link in seen:
                yield {
                    "STUDYID": record["STUDYID"],
                    "USUBJID": record["USUBJID"],
                    "RDOMAIN": domain,
                    "IDVAR": seqvar,
                    "IDVARVAL": str(record[seqvar]).strip(),
                    "RELTYPE": "",
                    "RELID": link,
                }


def main():
    rows = []
    rows.extend(rows_for("AE", "AESEQ", ["AELNKID1", "AELNKID2"],
                         os.path.join(INPUT_DIR, "ae.csv")))
    rows.extend(rows_for("CM", "CMSEQ", ["CMLNKID1", "CMLNKID2"],
                         os.path.join(INPUT_DIR, "cm.csv")))

    os.makedirs(OUTPUT_DIR, exist_ok=True)
    out_path = os.path.join(OUTPUT_DIR, "relrec.csv")
    with open(out_path, "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
