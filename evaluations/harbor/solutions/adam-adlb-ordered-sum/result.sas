/* SAS language reference solution for the yamaa benchmark adam-adlb-ordered-sum.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD LBTEST VISIT $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024. LBTEST : $char1024. VISIT : $char1024. LBSTRESN;
run;

data collected;
  length PARAMCD $16 PARAM $1024 AVISIT $1024 DTYPE $11;
  set lb; PARAMCD=LBTESTCD; PARAM=LBTEST; AVISIT=VISIT; AVAL=LBSTRESN;
run;
proc sort data=collected; by STUDYID USUBJID AVISIT LBSEQ; run;
data totals;
  length PARAMCD $16 PARAM $1024 DTYPE $11;
  set collected; by STUDYID USUBJID AVISIT;
  if first.AVISIT then do; total=0; count=0; end;
  if not missing(AVAL) then do; total+AVAL; count+1; end;
  if last.AVISIT then do;
    PARAMCD='TOTAL'; PARAM='Total of Components'; DTYPE='CALCULATION';
    AVAL=.; if count>0 then AVAL=total;
    output;
  end;
run;
data result; set collected totals; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, DTYPE from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
