/* opensas reference solution for the yamaa benchmark adam-adsl-site-parse.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID SUBJID SITEID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. SUBJID : $char1024. SITEID : $char1024.;
run;

data result;
  length SITEIDP SUBJREF $1024;
  set dm;
  if prxmatch('/^[^-]+-([^-]+)-[0-9]{4}$/',strip(USUBJID)) then SITEIDP=scan(USUBJID,2,'-');
  SUBJREF=catx(':',SITEID,SUBJID);
run;

proc sql;
  create table final as select STUDYID, USUBJID, SUBJID, SITEID, SITEIDP, SUBJREF from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
