/* opensas reference solution for the yamaa benchmark sdtm-dm-xpt.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm_raw;
  length STUDYID USUBJID SUBJID SITEID RFSTDTC SEX RACE COUNTRY $1024;
  infile "/app/input/dm_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. SUBJID : $char1024. SITEID : $char1024.
    RFSTDTC : $char1024. AGE SEX : $char1024. RACE : $char1024. COUNTRY : $char1024.;
run;

data result; set dm_raw; length DOMAIN $2 AGEU $8; DOMAIN='DM'; AGEU='YEARS'; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, SUBJID, RFSTDTC, SITEID, AGE, AGEU,
    SEX, RACE, COUNTRY from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
