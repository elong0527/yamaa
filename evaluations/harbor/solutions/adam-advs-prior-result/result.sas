/* SAS language reference solution for the yamaa benchmark adam-advs-prior-result.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data vs;
  length STUDYID USUBJID SERIES AVALC $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VSSEQ SERIES : $char1024. AVISITN AVALC : $char1024.;
run;

data ordered; set vs; ordinal=_n_; novisit=missing(AVISITN); run;
proc sort data=ordered; by STUDYID USUBJID SERIES novisit AVISITN ordinal; run;
data result;
  length PREVAVALC carried $1024;
  retain carried;
  set ordered; by STUDYID USUBJID SERIES;
  if first.SERIES then carried='';
  PREVAVALC=carried;
  if not missing(AVALC) then carried=AVALC;
run;

proc sql;
  create table final as select STUDYID, USUBJID, VSSEQ, SERIES, AVISITN, AVALC, PREVAVALC from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
