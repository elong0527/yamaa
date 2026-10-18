Following CDISC SDTM standards, use the provided DESIGN dataset to create a
TA dataset with one record per planned element per arm.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, ARMCD, ARM, TAETORD, ETCD, ELEMENT, EPOCH

"EXTENSION" is a sponsor epoch and is kept as written.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ta.csv.
