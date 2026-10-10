/* opensas reference solution for the yamaa benchmark adam-adrs-composite-response.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adrs_raw;
  length STUDYID USUBJID PARAMCD AVISIT $1024;
  infile "/app/input/adrs_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. AVISIT : $char1024. PCHG;
run;

data adsl;
  length STUDYID USUBJID SAEFL DCSREAS $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. SAEFL : $char1024. DCSREAS : $char1024.;
run;

proc sql; create table joined as select a.*,b.SAEFL,b.DCSREAS from adrs_raw as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result;
  length PARAM $40 AVALC $20 ARSN $40;
  set joined; PARAMCD='RESP75'; PARAM='EASI-75 Response';
  if SAEFL='Y' or not missing(DCSREAS) then do; AVALC='NON-RESPONDER'; ARSN='SAFETY OR DISCONTINUATION RULE'; AVAL=0; end;
  else if missing(PCHG) then do; AVALC='NOT EVALUABLE'; ARSN='COMPONENT MISSING'; end;
  else if PCHG<=-75 then do; AVALC='RESPONDER'; ARSN='THRESHOLD MET'; AVAL=1; end;
  else do; AVALC='NON-RESPONDER'; ARSN='THRESHOLD NOT MET'; AVAL=0; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, PCHG, SAEFL, DCSREAS, AVALC, ARSN, AVAL from result;
quit;
proc export data=final outfile="/app/output/adrs.csv" dbms=csv replace; run;
