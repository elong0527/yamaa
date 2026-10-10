/* opensas reference solution for the yamaa benchmark adam-adsl-age-group.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID AGEU $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AGE AGEU : $char1024.;
run;

data result;
  length AGEGR1 $7;
  set dm;
  if missing(AGE) then do; AGEGR1='Missing'; AGEGR1N=.; end;
  else if AGE < 18 then do; AGEGR1='<18'; AGEGR1N=1; end;
  else if AGE <= 64 then do; AGEGR1='18-64'; AGEGR1N=2; end;
  else do; AGEGR1='>64'; AGEGR1N=3; end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AGE, AGEU, AGEGR1, AGEGR1N from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
