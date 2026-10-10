/* opensas reference solution for the yamaa benchmark adam-adae-text-cleanup.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AESPID AETERM AEREL $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AESPID : $char1024. AETERM : $char1024. AEREL : $char1024.;
run;

data result;
  length AETERMLO AERELLC AREL $1024;
  set ae;
  if missing(AESPID) then AEREFNUM=0;
  else if prxmatch('/^AE-[0-9]{3}$/',strip(AESPID)) then AEREFNUM=input(substr(AESPID,4,3),best32.);
  else AEREFNUM=-1;
  AETERMLO=lowcase(AETERM); AERELLC=lowcase(AEREL);
  if missing(AEREL) then AERELLC='not reported';
  AREL=upcase(AERELLC);
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AESPID, AEREFNUM, AETERM, AETERMLO, AERELLC, AREL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
