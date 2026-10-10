/* opensas reference solution for the yamaa benchmark adam-adeg-derived-intervals.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adeg;
  length STUDYID USUBJID PARAMCD PARAM AVISIT AVALU $1024;
  infile "/app/input/adeg.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. PARAM : $char1024. AVISIT : $char1024. AVAL AVALU : $char1024.;
run;

proc sql;
  create table qt as select a.*,b.AVAL as RRVAL from adeg as a inner join adeg as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AVISIT=b.AVISIT where a.PARAMCD='QT' and b.PARAMCD='RR' and not missing(a.AVAL) and not missing(b.AVAL);
quit;
data corrected;
  length PARAMCD PARAM AVALU $1024;
  set qt;
  QTVAL=AVAL; AVALU='ms';
  PARAMCD='QTCBR'; PARAM="QTcB - Bazett's Correction Formula Rederived (ms)"; AVAL=QTVAL/sqrt(RRVAL/1000); output;
  PARAMCD='QTCFR'; PARAM="QTcF - Fridericia's Correction Formula Rederived (ms)"; AVAL=QTVAL/(RRVAL/1000)**(1/3); output;
run;
data heart;
  length PARAMCD PARAM $1024;
  set adeg;
  if PARAMCD='HR' and not missing(AVAL) and AVAL ne 0;
  AVAL=60000/AVAL; PARAMCD='RRR'; PARAM='RR Duration Rederived (ms)'; AVALU='ms';
run;
data result; set adeg corrected heart; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU from result;
quit;
proc export data=final outfile="/app/output/adeg.csv" dbms=csv replace; run;
