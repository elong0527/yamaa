# Reference solution for the yamaa benchmark adam-adcm-related-events (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
from pathlib import Path

INPUT_DIR = Path("/app/input")
OUTPUT_DIR = Path("/app/output")
RELAE_COLUMNS = ["STUDYID", "USUBJID", "RELID", "AESEQ", "AETERM"]
ADCM_COLUMNS = [
    "STUDYID",
    "USUBJID",
    "CMSEQ",
    "CMTRT",
    "AESEQS",
    "AETERMS",
    "AETERMN",
]


def read(name: str) -> list[dict[str, str]]:
    with open(INPUT_DIR / name, newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def write(name: str, columns: list[str], rows: list[dict]) -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_DIR / name, "w", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(columns)
        for row in rows:
            writer.writerow(["" if row[c] is None else row[c] for c in columns])


def main() -> None:
    ae = read("ae.csv")
    cm = read("cm.csv")
    relrec = read("relrec.csv")

    # Each adverse event a relationship names, with the event's reported term.
    terms = {(r["STUDYID"], r["USUBJID"], int(r["AESEQ"])): r["AETERM"] for r in ae}
    relae = []
    for r in relrec:
        if r["RDOMAIN"] == "AE" and r["IDVAR"] == "AESEQ":
            seq = int(r["IDVARVAL"])
            relae.append(
                {
                    "STUDYID": r["STUDYID"],
                    "USUBJID": r["USUBJID"],
                    "RELID": r["RELID"],
                    "AESEQ": seq,
                    "AETERM": terms[(r["STUDYID"], r["USUBJID"], seq)],
                }
            )
    relae.sort(key=lambda r: (r["STUDYID"], r["USUBJID"], r["RELID"], r["AESEQ"]))

    # The relationship each medication belongs to.
    relationship = {
        (r["STUDYID"], r["USUBJID"], r["IDVARVAL"]): r["RELID"]
        for r in relrec
        if r["RDOMAIN"] == "CM" and r["IDVAR"] == "CMSEQ"
    }

    adcm = []
    for r in cm:
        relid = relationship.get((r["STUDYID"], r["USUBJID"], r["CMSEQ"]))
        related = [
            event
            for event in relae
            if relid is not None
            and (event["STUDYID"], event["USUBJID"], event["RELID"])
            == (r["STUDYID"], r["USUBJID"], relid)
        ]
        sequences = sorted(event["AESEQ"] for event in related)
        names = sorted({event["AETERM"] for event in related})
        adcm.append(
            {
                "STUDYID": r["STUDYID"],
                "USUBJID": r["USUBJID"],
                "CMSEQ": int(r["CMSEQ"]),
                "CMTRT": r["CMTRT"],
                "AESEQS": ", ".join(str(s) for s in sequences) or None,
                "AETERMS": ", ".join(names) or None,
                "AETERMN": len(names) or None,
            }
        )
    adcm.sort(key=lambda r: (r["STUDYID"], r["USUBJID"], r["CMSEQ"]))

    write("relae.csv", RELAE_COLUMNS, relae)
    write("adcm.csv", ADCM_COLUMNS, adcm)


if __name__ == "__main__":
    main()
