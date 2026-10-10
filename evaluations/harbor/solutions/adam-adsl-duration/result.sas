/* SAS language reference solution for the yamaa benchmark adam-adsl-duration.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID STDT ENDT $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. STDT : $char1024. ENDT : $char1024.;
run;

data result;
  set dm;
  start=input(STDT,yymmdd10.); finish=input(ENDT,yymmdd10.);
  if not missing(start) and not missing(finish) then do;
    lo=min(start,finish); hi=max(start,finish); sign=1;
    if finish<start then sign=-1;
  DURW=sign*floor((hi-lo)/7); DURM=sign*intck('month',lo,hi,'continuous');
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, STDT, ENDT, DURW, DURM from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
