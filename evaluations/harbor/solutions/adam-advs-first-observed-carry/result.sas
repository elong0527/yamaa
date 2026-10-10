/* opensas reference solution for the yamaa benchmark adam-advs-first-observed-carry.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data vs;
  length STUDYID USUBJID PARAMCD $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. AVISITN AVAL;
run;

proc sort data=vs out=ordered; by STUDYID USUBJID PARAMCD AVISITN; run;
data result;
  set ordered; by STUDYID USUBJID PARAMCD;
  retain carried;
  if first.PARAMCD then carried=.;
  if not missing(AVAL) and missing(carried) then carried=AVAL;
  BASEVAL=carried;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, BASEVAL from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
