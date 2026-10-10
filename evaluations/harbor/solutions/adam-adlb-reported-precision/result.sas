/* SAS language reference solution for the yamaa benchmark adam-adlb-reported-precision.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID PARAMCD $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. AVAL ANRLO;
run;

data result;
  set lb;
  if not missing(AVAL) and not missing(ANRLO) and ANRLO ne 0 then R2ANRLO=round(AVAL/ANRLO,0.0001);
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, R2ANRLO from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
