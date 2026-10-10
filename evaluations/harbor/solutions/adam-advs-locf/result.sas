/* SAS language reference solution for the yamaa benchmark adam-advs-locf.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data vs;
  length USUBJID PARAMCD $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input USUBJID : $char1024. PARAMCD : $char1024. AVISITN AVALCOL;
run;

proc sort data=vs out=ordered; by USUBJID PARAMCD AVISITN; run;
data result;
  set ordered; by USUBJID PARAMCD;
  retain carried;
  if first.PARAMCD then carried=.;
  if not missing(AVALCOL) then carried=AVALCOL;
  AVAL=carried;
run;

proc sql;
  create table final as select USUBJID, PARAMCD, AVISITN, AVAL from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
