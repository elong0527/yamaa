/* SAS language reference solution for the yamaa benchmark adam-adae-severity-override.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AESEV $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AESEV : $char1024.;
run;

data graded;
  length ASEV $20;
  set ae; ASEV=AESEV;
  ASEV=upcase(AESEV);
  if (USUBJID='CATH-01-001' and AESEQ=2) or (USUBJID='CATH-01-003' and AESEQ=1) then ASEV='SEVERE';
  select(ASEV); when('MILD') ASEVN=1; when('MODERATE') ASEVN=2; when('SEVERE') ASEVN=3; when('LIFE-THREATENING') ASEVN=4; otherwise ASEVN=.; end;
run;
data result; set graded; run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, ASEV, ASEVN from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
