/* SAS language reference solution for the yamaa benchmark sdtm-dm-metadata.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data dm_raw;
  length STUDYID SUBJID SITEID SEX COUNTRY $1024;
  infile "/app/input/dm_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. SUBJID : $char1024. SITEID : $char1024. AGE SEX : $char1024. COUNTRY : $char1024.;
run;

data result; set dm_raw; length DOMAIN $2 USUBJID $1024 AGEU $8; DOMAIN='DM'; USUBJID=catx('-',STUDYID,SITEID,SUBJID); AGEU='YEARS'; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, COUNTRY from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
