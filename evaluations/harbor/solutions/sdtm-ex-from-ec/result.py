# Reference solution for the yamaa benchmark sdtm-ex-from-ec (Python track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

"""Build the SDTM EX dataset from the ODM extract and the kit list.

One record per administration: an EC form occurrence whose occurrence
item (IT.EC.ECOCCUR) is "Y". Dose derivation depends on the collected
form:
- TABLET:   number of tablets times tablet strength.
- INFUSION: collected mg/kg dose times body weight at the same visit.
- KITDOSE:  treatment and dose from the kit list for the kit number
            (a listed dose of zero stays zero).
- AUCDOSE:  the AUC target as collected; unit "AUC" instead of "mg".
EXSEQ numbers each subject's records from 1 by start date.
"""

import csv
from collections import OrderedDict

INPUT_DIR = "/app/input"
OUTPUT_DIR = "/app/output"


def num(text):
    value = float(text)
    return int(value) if value == int(value) else value


def fmt(value):
    if value is None:
        return ""
    if isinstance(value, float) and value == int(value):
        return str(int(value))
    return str(value)


def main():
    with open(f"{INPUT_DIR}/kit_list.csv", newline="") as fh:
        kits = {row["KIT"]: row for row in csv.DictReader(fh)}

    with open(f"{INPUT_DIR}/odm.csv", newline="") as fh:
        odm = list(csv.DictReader(fh))

    study_id = odm[0]["StudyOID"]

    # Group EC form items per form occurrence; collect visit weights.
    forms = OrderedDict()
    weights = {}
    for row in odm:
        visit_key = (row["SubjectKey"], row["StudyEventOID"],
                     row["StudyEventRepeatKey"])
        if row["ItemOID"] == "IT.VS.WEIGHT":
            weights[visit_key] = num(row["Value"])
        if row["FormOID"] != "FO.EC":
            continue
        form_key = visit_key + (row["FormRepeatKey"],
                                row["ItemGroupRepeatKey"])
        forms.setdefault(form_key, {})[row["ItemOID"]] = row["Value"]

    records = []
    for (subject, *_rest, _fr, _igr), items in forms.items():
        if items.get("IT.EC.ECOCCUR") != "Y":
            continue
        form = items.get("IT.EC.FORM", "")
        start = items.get("IT.EC.ECSTDTC", "")
        end = items.get("IT.EC.ECENDTC", "")
        if form == "KITDOSE":
            kit = kits[items["IT.EC.KIT"]]
            treatment = kit["TREATMENT"]
            dose = num(kit["DOSE_MG"])
        else:
            treatment = items.get("IT.EC.ECTRT", "")
            if form == "TABLET":
                dose = num(items["IT.EC.TABLETS"]) * num(
                    items["IT.EC.STRENGTHMG"])
            elif form == "INFUSION":
                visit_key = (subject, _rest[0], _rest[1])
                dose = num(items["IT.EC.DOSEMKG"]) * weights[visit_key]
            elif form == "AUCDOSE":
                dose = num(items["IT.EC.AUCTARGET"])
            else:
                dose = None
        unit = "AUC" if form == "AUCDOSE" else "mg"
        records.append({
            "DOMAIN": "EX",
            "STUDYID": study_id,
            "USUBJID": subject,
            "EXTRT": treatment,
            "EXDOSE": dose,
            "EXDOSU": unit,
            "EXSTDTC": start,
            "EXENDTC": end,
        })

    records.sort(key=lambda r: (r["USUBJID"], r["EXSTDTC"]))
    counters = {}
    for rec in records:
        counters[rec["USUBJID"]] = counters.get(rec["USUBJID"], 0) + 1
        rec["EXSEQ"] = counters[rec["USUBJID"]]

    columns = ["DOMAIN", "STUDYID", "USUBJID", "EXSEQ", "EXTRT",
               "EXDOSE", "EXDOSU", "EXSTDTC", "EXENDTC"]
    with open(f"{OUTPUT_DIR}/ex.csv", "w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=columns)
        writer.writeheader()
        for rec in records:
            writer.writerow({c: fmt(rec[c]) if c == "EXDOSE" else rec[c]
                             for c in columns})


if __name__ == "__main__":
    main()
