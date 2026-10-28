import csv
from decimal import Decimal
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")
PARAMETERS = {
    "Cmax": ("CMAX", "Max Conc", "ng/mL"),
    "Tmax": ("TMAX", "Time of CMAX Observation", "h"),
    "AUClast": ("AUCLST", "AUC to Last Nonzero Conc", "h*ng/mL"),
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
    with (INPUT / "nca_transfer.csv").open(newline="", encoding="utf-8-sig") as source:
        supplied = list(csv.DictReader(source))
    supplied.sort(
        key=lambda r: (
            r["Protocol"],
            r["Subject"],
            r["ReferenceDose"],
            r["Analyte"],
            PARAMETERS[r["Parameter"]][0],
        )
    )
    sequences = {}
    OUTPUT.mkdir(exist_ok=True)
    with (OUTPUT / "pp.csv").open("w", newline="", encoding="utf-8") as target:
        writer = csv.DictWriter(target, fieldnames=COLUMNS)
        writer.writeheader()
        for row in supplied:
            code, label, unit = PARAMETERS[row["Parameter"]]
            subject = (row["Protocol"], row["Subject"])
            sequences[subject] = sequences.get(subject, 0) + 1
            estimate = row["Estimate"]
            numeric_estimate = Decimal(estimate)
            writer.writerow(
                dict(
                    zip(
                        COLUMNS,
                        [
                            "PP",
                            row["Protocol"],
                            row["Subject"],
                            sequences[subject],
                            code,
                            label,
                            row["Analyte"],
                            estimate,
                            unit,
                            estimate,
                            numeric_estimate,
                            unit,
                            "PLASMA",
                            row["ReferenceDose"],
                        ],
                    )
                )
            )


if __name__ == "__main__":
    main()
