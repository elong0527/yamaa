# Reference solution for the yamaa benchmark sdtm-vs-diary-missed-days (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

import csv
import datetime as dt
from pathlib import Path

INPUT = Path("/app/input")
OUTPUT = Path("/app/output")

COLUMNS = [
    "DOMAIN", "STUDYID", "USUBJID", "VSSEQ", "VSTESTCD", "VSTEST", "VSORRES",
    "VSORRESU", "VSSTAT", "VSREASND", "VSDTC", "VSTPT", "VSTPTNUM", "VSTPTREF",
    "VSRFTDTC",
]


def read(name):
    with open(INPUT / name, newline="", encoding="utf-8") as handle:
        return [
            {k.strip(): (v or "").strip() for k, v in row.items()}
            for row in csv.DictReader(handle)
        ]


def main():
    days = sorted(read("diary_days.csv"), key=lambda r: int(r["DIARYDAY"]))
    temps = {}
    for row in read("temperature.csv"):
        temps.setdefault((row["USUBJID"], row["TEMPDAT"]), row)

    given = [v for v in read("vaccination.csv") if v["VACDAT"]]
    given.sort(key=lambda v: (v["USUBJID"], v["VACDAT"], int(v["DOSENUM"] or 0)))

    records = []
    seq = {}
    for vac in given:
        start = dt.date.fromisoformat(vac["VACDAT"])
        for day in days:
            num = int(day["DIARYDAY"])
            date = (start + dt.timedelta(days=num - 1)).isoformat()
            temp = temps.get((vac["USUBJID"], date))
            seq[vac["USUBJID"]] = seq.get(vac["USUBJID"], 0) + 1
            records.append({
                "DOMAIN": "VS",
                "STUDYID": vac["STUDYID"],
                "USUBJID": vac["USUBJID"],
                "VSSEQ": seq[vac["USUBJID"]],
                "VSTESTCD": "TEMP",
                "VSTEST": "Temperature",
                "VSORRES": temp["TEMP"] if temp else "",
                "VSORRESU": temp["TEMPU"] if temp else "",
                "VSSTAT": "" if temp else "NOT DONE",
                "VSREASND": "" if temp else "NOT ENTERED",
                "VSDTC": date,
                "VSTPT": day["DAYLABEL"],
                "VSTPTNUM": num,
                "VSTPTREF": vac["VISIT"],
                "VSRFTDTC": vac["VACDAT"],
            })

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT / "vs.csv", "w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(records)


if __name__ == "__main__":
    main()
