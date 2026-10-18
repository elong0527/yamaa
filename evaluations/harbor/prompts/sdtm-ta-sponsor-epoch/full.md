Following CDISC SDTM standards, use the provided DESIGN dataset to create a
TA dataset with one record per planned element per arm.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, ARMCD, ARM, TAETORD, ETCD, ELEMENT, EPOCH

ARMCD, ARM, ETCD, ELEMENT and EPOCH are as the protocol design states them,
and TAETORD is the element's planned order within its arm, starting at 1.
"EXTENSION", the open-label extension period, is a sponsor addition to the
published epoch terms and is kept as written.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ta.csv.
