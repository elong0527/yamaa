import csv
from decimal import Decimal
from pathlib import Path

input_dir = Path("/app/input")
output_dir = Path("/app/output")
output_dir.mkdir(exist_ok=True)
columns = [
    "DOMAIN",
    "STUDYID",
    "TSSEQ",
    "TSPARMCD",
    "TSPARM",
    "TSVAL",
    "TSVALCD",
    "TSVCDREF",
]
with (input_dir / "protocol.csv").open(newline="", encoding="utf-8-sig") as source:
    protocols = list(csv.DictReader(source))
rows = []
for protocol in protocols:
    study = protocol["Study"]
    phase_value, phase_code = {"Phase 2": ("PHASE II TRIAL", "C15601")}[
        protocol["Phase"]
    ]
    random_value, random_code = {"Yes": ("Y", "C49488")}[protocol["Randomized"]]
    planned = Decimal(protocol["PlannedSubjects"])
    if planned != planned.to_integral_value():
        raise ValueError("PlannedSubjects must be an integer")
    parameters = [
        (1, "TITLE", "Trial Title", protocol["Title"], "", ""),
        (2, "TPHASE", "Trial Phase Classification", phase_value, phase_code, "CDISC"),
        (3, "RANDOM", "Trial is Randomized", random_value, random_code, "CDISC"),
        (4, "PLANSUB", "Planned Number of Subjects", str(int(planned)), "", ""),
    ]
    for parameter in parameters:
        rows.append(["TS", study, *parameter])
with (output_dir / "ts.csv").open("w", newline="", encoding="utf-8") as target:
    writer = csv.writer(target)
    writer.writerow(columns)
    writer.writerows(rows)
