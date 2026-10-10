/* SAS language reference solution for the yamaa benchmark adam-adae-query-flags.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AEDECOD $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AEDECOD : $char1024.;
run;

data queries;
  length PREFIX GRPNAME GRPID SCOPE TERM $1024;
  infile "/app/input/queries.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input PREFIX : $char1024. GRPNAME : $char1024. GRPID : $char1024. SCOPE : $char1024. TERM : $char1024.;
run;

proc sql;
  create table result as select a.*,
    b.GRPNAME as SMQ01NAM,b.GRPID as SMQ01CD,b.SCOPE as SMQ01SC,
    c.GRPNAME as SMQ02NAM,c.GRPID as SMQ02CD,c.SCOPE as SMQ02SC,
    d.GRPNAME as CQ01NAM
  from ae as a left join queries as b on a.AEDECOD=b.TERM and b.PREFIX='SMQ01' and a.AEDECOD ne ''
    left join queries as c on a.AEDECOD=c.TERM and c.PREFIX='SMQ02' and a.AEDECOD ne ''
    left join queries as d on a.AEDECOD=d.TERM and d.PREFIX='CQ01' and a.AEDECOD ne '';
quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, SMQ01NAM, SMQ01CD, SMQ01SC, SMQ02NAM, SMQ02CD, SMQ02SC, CQ01NAM from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
