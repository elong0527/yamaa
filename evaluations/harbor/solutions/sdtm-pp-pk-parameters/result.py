import csv
from datetime import datetime
from decimal import Decimal
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")
PARAMETERS = {
    "Cmax": ("CMAX", "Max Conc"),
    "Tmax": ("TMAX", "Time of CMAX Observation"),
    "AUClast": ("AUCLST", "AUC to Last Nonzero Conc"),
}
COLUMNS = [
    "DOMAIN",
    "STUDYID",
    "USUBJID",
    "PPSEQ",
    "PPTESTCD",
    "PPTEST",
    "PPCAT",
    "PPORRES",
    "PPORRESU",
    "PPSTRESC",
    "PPSTRESN",
    "PPSTRESU",
    "PPSPEC",
    "PPRFTDTC",
]


def main():
    records = []
    with (INPUT / "nca_transfer.csv").open(newline="", encoding="utf-8-sig") as source:
        for row in csv.DictReader(source):
            code, name = PARAMETERS[row["Parameter"]]
            estimate = row["Estimate"]
            records.append(
                {
                    "DOMAIN": "PP",
                    "STUDYID": row["Protocol"],
                    "USUBJID": row["Subject"],
                    "PPTESTCD": code,
                    "PPTEST": name,
                    "PPCAT": row["Analyte"],
                    "PPORRES": estimate,
                    "PPORRESU": row["Unit"],
                    "PPSTRESC": estimate,
                    "PPSTRESN": format(Decimal(estimate), "f"),
                    "PPSTRESU": row["Unit"],
                    "PPSPEC": "PLASMA",
                    "PPRFTDTC": datetime.fromisoformat(row["ReferenceDose"]).strftime(
                        "%Y-%m-%dT%H:%M:%S"
                    ),
                }
            )
    records.sort(
        key=lambda row: (
            row["STUDYID"],
            row["USUBJID"],
            row["PPRFTDTC"],
            row["PPCAT"],
            row["PPTESTCD"],
        )
    )
    previous = None
    sequence = 0
    for row in records:
        subject = (row["STUDYID"], row["USUBJID"])
        sequence = sequence + 1 if subject == previous else 1
        row["PPSEQ"] = sequence
        previous = subject
    OUTPUT.mkdir(exist_ok=True)
    with (OUTPUT / "pp.csv").open("w", newline="", encoding="utf-8") as target:
        writer = csv.DictWriter(target, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
