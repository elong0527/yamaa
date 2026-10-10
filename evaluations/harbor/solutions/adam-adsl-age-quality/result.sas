/* SAS language reference solution for the yamaa benchmark adam-adsl-age-quality.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AGE;
run;

proc sql;
  create table result as
  select STUDYID, USUBJID, AGE from dm;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, AGE from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
