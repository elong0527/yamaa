/* opensas reference solution for the yamaa benchmark sdtm-dm-no-deaths.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm_raw;
  length STUDYID USUBJID RFSTDTC DTHDAT $1024;
  infile "/app/input/dm_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RFSTDTC : $char1024. DTHDAT : $char1024.;
run;

data result;
  set dm_raw;
  length DOMAIN $2 DTHDTC $1024 DTHFL $1;
  DOMAIN = 'DM';
  DTHDTC = DTHDAT;
  if not missing(DTHDTC) then DTHFL = 'Y';
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, RFSTDTC, DTHDTC, DTHFL from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
