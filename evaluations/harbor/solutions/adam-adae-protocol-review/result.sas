/* SAS language reference solution for the yamaa benchmark adam-adae-protocol-review.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AESTDTC AESTDTM AETERM SCORE $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AESTDTC : $char1024. AESTDTM : $char1024. AETERM : $char1024. SCORE : $char1024.;
run;

data result;
  length ASTDT ASTDT2 $10 REVIEWFL $1;
  set ae;
  ASTDT=AESTDTC; ASTDT2=''; if lengthn(AESTDTM)>=10 then ASTDT2=substr(AESTDTM,1,10);
  REVIEWFL='N';
  INFTERM=0;
  if lengthn(AETERM)>=4 then INFTERM=(substr(AETERM,1,4)='INF_');
  if ((AESTDTC>='2025-01-01' and AESTDTC<='2025-01-31') or AESTDTM>='2025-02-01T09:30') and INFTERM=1 and not missing(SCORE) and input(SCORE,?? best32.)>=-1.5 then REVIEWFL='Y';
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, ASTDT, ASTDT2, REVIEWFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
