/* SAS language reference solution for the yamaa benchmark sdtm-ex-combination.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ex_raw;
  length STUDYID USUBJID EXTRT EXDOSU EXSTDTC EXENDTC EXADJ $1024;
  infile "/app/input/ex_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXTRT : $char1024. EXDOSE EXDOSU : $char1024. EXSTDTC : $char1024. EXENDTC : $char1024. EXADJ : $char1024.;
run;

data prepared; set ex_raw; length DOMAIN $2; DOMAIN='EX'; run;
proc sort data=prepared; by STUDYID USUBJID EXSTDTC EXTRT; run;
data result; set prepared; by STUDYID USUBJID; retain EXSEQ;
  if first.USUBJID then EXSEQ=0;
  EXSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC, EXADJ from result;
quit;
proc export data=final outfile="/app/output/ex.csv" dbms=csv replace; run;
