/* opensas reference solution for the yamaa benchmark adam-adsl-analysis-age.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID BRTHDT RANDDT $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. BRTHDT : $char1024. RANDDT : $char1024.;
run;

data result;
  set dm;
  start=input(BRTHDT,yymmdd10.); finish=input(RANDDT,yymmdd10.);
  if not missing(start) and not missing(finish) then do;
    lo=min(start,finish); hi=max(start,finish); sign=1;
    if finish<start then sign=-1;
  AAGE=sign*intck('year',lo,hi,'continuous');
  end;
  length AAGEU $5; AAGEU='YEARS';
run;

proc sql;
  create table final as select STUDYID, USUBJID, BRTHDT, RANDDT, AAGE, AAGEU from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
