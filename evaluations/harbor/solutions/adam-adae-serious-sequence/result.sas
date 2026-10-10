/* SAS language reference solution for the yamaa benchmark adam-adae-serious-sequence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AESER AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESER : $char1024. AESTDTC : $char1024.;
run;

data ordered; set ae; nodate=missing(AESTDTC); run;
proc sort data=ordered; by STUDYID USUBJID nodate AESTDTC AESEQ; run;
data result;
  length ASTDT $10;
  set ordered; by STUDYID USUBJID;
  if first.USUBJID then counter=0;
  ASTDT=AESTDTC;
  if AESER='Y' then do; counter+1; SERSEQ=counter; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AESER, ASTDT, SERSEQ from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
