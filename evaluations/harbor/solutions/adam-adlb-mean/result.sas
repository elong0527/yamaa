/* opensas reference solution for the yamaa benchmark adam-adlb-mean.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024. LBSTRESN;
run;

data collected;
  length PARAMCD $1024 DTYPE $7;
  set lb;
  PARAMCD=LBTESTCD; AVAL=LBSTRESN; DTYPE=''; ORD=0;
run;

proc sql;
  create table averages as
  select STUDYID, USUBJID, LBTESTCD as PARAMCD length=1024, . as LBSEQ,
         mean(LBSTRESN) as AVAL, 'AVERAGE' as DTYPE length=7, 1 as ORD
  from lb where not missing(LBSTRESN)
  group by STUDYID, USUBJID, LBTESTCD;
quit;

data both; set collected averages; run;
proc sort data=both; by STUDYID USUBJID PARAMCD ORD LBSEQ; run;
data result;
  set both; by STUDYID USUBJID;
  if first.USUBJID then ASEQ=0;
  ASEQ+1;
run;
proc sort data=result; by USUBJID ASEQ; run;

proc sql;
  create table final as
  select STUDYID, USUBJID, ASEQ, LBSEQ, PARAMCD, AVAL, DTYPE from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
