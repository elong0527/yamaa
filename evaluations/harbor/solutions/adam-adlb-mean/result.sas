/* SAS language reference solution for the yamaa benchmark adam-adlb-mean.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024. LBSTRESN;
run;

proc sql;
  create table result as
  select a.*,a.LBTESTCD as PARAMCD,a.LBSTRESN as AVAL,b.AVALMEAN from lb as a left join (select STUDYID,USUBJID,LBTESTCD,mean(LBSTRESN) as AVALMEAN from lb group by STUDYID,USUBJID,LBTESTCD) as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.LBTESTCD=b.LBTESTCD;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, LBSEQ, PARAMCD, AVAL, AVALMEAN from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
