/* opensas reference solution for the yamaa benchmark adam-adlb-parameter-names.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024.
    LBSTRESN VISITNUM;
run;

data result;
  set lb;
  length PARAMCD $8 PARAM $40 AVISIT $20;
  PARAMCD = LBTESTCD;
  if LBTESTCD = 'ALT' then PARAM = 'Alanine Aminotransferase (U/L)';
  else if LBTESTCD = 'AST' then PARAM = 'Aspartate Aminotransferase (U/L)';
  AVISITN = VISITNUM;
  if AVISITN = 1 then AVISIT = 'Baseline';
  else if AVISITN = 2 then AVISIT = 'Week 2';
  else AVISIT = 'Unscheduled';
  AVAL = LBSTRESN;
run;
proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVISITN,
    AVISIT, AVAL
    from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
