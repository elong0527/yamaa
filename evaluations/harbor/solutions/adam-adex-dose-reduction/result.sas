/* opensas reference solution for the yamaa benchmark adam-adex-dose-reduction.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ex;
  length STUDYID USUBJID EXSTDTM $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXSTDTM : $char1024. EXDOSE;
run;

proc sort data=ex out=ordered; by STUDYID USUBJID EXSTDTM EXSEQ; run;
data result;
  length DOSREDFL $1;
  retain previous;
  set ordered; by STUDYID USUBJID;
  if first.USUBJID then previous=.;
  if EXDOSE>0 and previous>0 and EXDOSE<previous then DOSREDFL='Y';
  previous=EXDOSE;
run;

proc sql;
  create table final as select STUDYID, USUBJID, EXSEQ, EXSTDTM, EXDOSE, DOSREDFL from result;
quit;
proc export data=final outfile="/app/output/adex.csv" dbms=csv replace; run;
