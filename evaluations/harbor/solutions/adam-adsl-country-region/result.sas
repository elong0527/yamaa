/* opensas reference solution for the yamaa benchmark adam-adsl-country-region.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID COUNTRY $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. COUNTRY : $char1024.;
run;

data result;
  length REGION1 $20;
  set dm;
  COUNTRY=upcase(COUNTRY);
  if missing(COUNTRY) then COUNTRY='UNKNOWN';
  if COUNTRY in ('USA','CAN') then REGION1='North America';
  else if COUNTRY='DEU' then REGION1='Europe';
  else REGION1='Rest of World';
run;

proc sql;
  create table final as select STUDYID, USUBJID, COUNTRY, REGION1 from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
