/* SAS language reference solution for the yamaa benchmark sdtm-lb-grading.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data lb_raw;
  length STUDYID USUBJID LBTESTCD SEX REPROGRADE $1024;
  infile "/app/input/lb_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024. SEX : $char1024. LBSTRESN REPROGRADE : $char1024.;
run;

data result; set lb_raw; length DOMAIN $2; DOMAIN='LB'; LBTOXGR=.;
if LBTESTCD='ANC' then do; if LBSTRESN<0.5 then LBTOXGR=4; else if LBSTRESN<1 then LBTOXGR=3; else if LBSTRESN<1.5 then LBTOXGR=2; else if LBSTRESN<1.8 then LBTOXGR=1; else LBTOXGR=0; end;
else if LBTESTCD='HGB' and SEX in ('M','F') then do; if LBSTRESN<8 then LBTOXGR=3; else if LBSTRESN<10 then LBTOXGR=2; else if (SEX='M' and LBSTRESN<13.5) or (SEX='F' and LBSTRESN<12) then LBTOXGR=1; else LBTOXGR=0; end; else delete; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBTOXGR from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
