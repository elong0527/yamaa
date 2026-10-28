from pathlib import Path

import pandas as pd


def main():
    source = pd.read_csv(
        "/app/input/lab_transfer.csv", dtype=str, keep_default_na=False
    )
    source["_collection_time"] = pd.to_datetime(
        source["Collected"], errors="raise", utc=True
    )
    source = source.sort_values(
        ["Protocol", "Subject", "_collection_time", "Sample"], kind="stable"
    )
    result = pd.DataFrame(index=source.index)
    result["DOMAIN"] = "PC"
    result["STUDYID"] = source["Protocol"]
    result["USUBJID"] = source["Subject"]
    result["PCSEQ"] = source.groupby(["Protocol", "Subject"], sort=False).cumcount() + 1
    result["PCREFID"] = source["Sample"]
    result["PCTESTCD"] = "ZX201"
    result["PCTEST"] = "ZX-201"
    result["PCORRES"] = source["Result"]
    result["PCORRESU"] = source["Unit"]
    result["PCSTRESC"] = source["Result"]
    # Preserve the reported numeric precision in the CSV, with BLQ left empty.
    result["PCSTRESN"] = source["Result"].where(source["Result"].ne("BLQ"), "")
    result["PCSTRESU"] = source["Unit"]
    result["PCSPEC"] = "PLASMA"
    result["PCDTC"] = source["Collected"]
    result["PCTPT"] = source["Nominal"]

    def planned_hours(label):
        if label == "PRE":
            return "0"
        if label == "UNSCHED":
            return ""
        if label.endswith("H"):
            hours = label[:-1]
            float(hours)
            return hours
        raise ValueError(f"Unexpected nominal label: {label}")

    result["PCTPTNUM"] = source["Nominal"].map(planned_hours)
    Path("/app/output").mkdir(exist_ok=True)
    result.to_csv("/app/output/pc.csv", index=False)


if __name__ == "__main__":
    main()
