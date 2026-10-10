/* SAS language reference solution for the yamaa benchmark adam-adrs-confirmed-response.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data rs;
  length STUDYID USUBJID RSDTC RSSTRESC $1024;
  infile "/app/input/rs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RSSEQ RSDTC : $char1024. RSSTRESC : $char1024.;
run;

proc sort data=rs out=ordered; by STUDYID USUBJID descending RSDTC descending RSSEQ; run;
data result;
  length PARAMCD $16 ADT $10 AVALC $1024 CONFIRMED $1 nextresult $1024 nextdate $10;
  retain nextresult nextdate;
  set ordered; by STUDYID USUBJID;
  if first.USUBJID then do; nextresult=''; nextdate=''; end;
  PARAMCD='CONFRESP'; ADT=RSDTC; AVALC=RSSTRESC; CONFIRMED='N';
  if AVALC='PD' then CONFIRMED='Y';
  else if AVALC in ('PR','CR') and nextresult in ('PR','CR') and not missing(ADT) and not missing(nextdate) and input(nextdate,yymmdd10.)-input(ADT,yymmdd10.)>=28 then CONFIRMED='Y';
  nextresult=AVALC; nextdate=ADT;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, RSSEQ, ADT, AVALC, CONFIRMED from result;
quit;
proc export data=final outfile="/app/output/adrs.csv" dbms=csv replace; run;
