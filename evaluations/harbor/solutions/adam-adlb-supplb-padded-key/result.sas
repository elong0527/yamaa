/* opensas reference solution for the yamaa benchmark adam-adlb-supplb-padded-key.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ;
run;

data supplb;
  length STUDYID USUBJID IDVARVAL QVAL $1024;
  infile "/app/input/supplb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. IDVARVAL : $char1024. QVAL : $char1024.;
run;

data lb_key; set lb; length PARENTKEY $8; PARENTKEY=put(LBSEQ,8.); run;
proc sql;
  create table result as
  select a.*,b.QVAL,b.QVAL as QVAL_NAMED from lb_key as a left join supplb as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARENTKEY=b.IDVARVAL;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, LBSEQ, QVAL, QVAL_NAMED from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
