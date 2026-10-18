Following CDISC SDTM standards, use the provided AE_RAW and MEDDRA_SYNTHETIC
datasets to create an AE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AELLT, AELLTCD, AEDECOD, AEPTCD,
AEHLT, AEHLTCD, AEHLGT, AEHLGTCD, AEBODSYS, AEBODSCD, AESOC, AESOCCD

DOMAIN is "AE". AETERM is the event text as reported, even when it
differs from the dictionary name held in AELLT. AELLTCD is the
lowest-level term code the medical coder assigned to the event, and
AELLT is the dictionary name of that code in MEDDRA_SYNTHETIC. The
coded hierarchy comes from the row of MEDDRA_SYNTHETIC that holds the
record's AELLTCD on the primary path, the row whose PRIMARY_SOC is
"Y". An LLT code can also have secondary paths, with the same
preferred term but a different high-level term, high-level group
term, and system organ class, and a secondary row can appear before
the primary row in the extract; no hierarchy value comes from a
secondary path. AEDECOD and AEPTCD are the preferred-term name and
code of the primary path, AEHLT and AEHLTCD are its high-level term
name and code, and AEHLGT and AEHLGTCD are its high-level group term
name and code. AEBODSYS and AEBODSCD are the system organ class name
and code of the primary path, and AESOC and AESOCCD repeat that same
name and code. When the record's AELLTCD has no value or no row in
MEDDRA_SYNTHETIC, AELLT and every hierarchy column have no value;
AELLTCD keeps the assigned code as collected.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
