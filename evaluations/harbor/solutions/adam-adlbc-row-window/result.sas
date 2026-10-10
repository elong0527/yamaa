/* opensas reference solution for the yamaa benchmark adam-adlbc-row-window.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBTESTCD : $char1024. LBSTRESN VISITNUM;
run;

data collected;
  length PARAMCD $8;
  set lb; if LBTESTCD in ('ALB','BILI') and not missing(LBSTRESN);
  PARAMCD=cats('_',LBTESTCD); AVISITN=VISITNUM; AVAL=LBSTRESN;
run;
proc sort data=collected; by STUDYID USUBJID PARAMCD AVISITN; run;
data result;
  retain previous priorchange;
  set collected; by STUDYID USUBJID PARAMCD;
  if first.PARAMCD then do; previous=.; priorchange=.; end;
  PREV_AVAL=previous; CHG=AVAL-PREV_AVAL; PREV2=priorchange;
  previous=AVAL; priorchange=CHG;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG from result;
quit;
proc export data=final outfile="/app/output/adlbc.csv" dbms=csv replace; run;
