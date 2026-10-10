/* SAS language reference solution for the yamaa benchmark adam-adlb-end-of-treatment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024. VISITNUM LBSTRESN;
run;

data supplb;
  length STUDYID RDOMAIN USUBJID IDVAR IDVARVAL QNAM QLABEL QVAL $1024;
  infile "/app/input/supplb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. RDOMAIN : $char1024. USUBJID : $char1024. IDVAR : $char1024. IDVARVAL : $char1024. QNAM : $char1024. QLABEL : $char1024. QVAL : $char1024.;
run;

data supplb_key; set supplb; PARENTSEQ=input(IDVARVAL,?? best32.); run;
proc sql;
  create table joined as select a.*,a.LBSTRESN as AVAL,b.QVAL as ENDPOINT from lb as a left join supplb_key as b
    on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and b.RDOMAIN='LB' and b.IDVAR='LBSEQ' and a.LBSEQ=b.PARENTSEQ and b.QNAM='ENDPOINT';
quit;
data ordered; set joined; priority=0; if ENDPOINT='Y' then priority=1; run;
proc sort data=ordered; by STUDYID USUBJID LBTESTCD descending priority descending VISITNUM LBSEQ; run;
data result; length EOTFL $1; set ordered; by STUDYID USUBJID LBTESTCD; if first.LBTESTCD then EOTFL='Y'; run;

proc sql;
  create table final as select STUDYID, USUBJID, LBSEQ, LBTESTCD, VISITNUM, AVAL, ENDPOINT, EOTFL from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
