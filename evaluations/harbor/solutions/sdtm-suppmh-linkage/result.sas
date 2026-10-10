/* SAS language reference solution for the yamaa benchmark sdtm-suppmh-linkage.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data mh;
  length STUDYID USUBJID MHTERM $1024;
  infile "/app/input/mh.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. MHSEQ MHTERM : $char1024.;
run;

data mh_supp_raw;
  length STUDYID USUBJID MHTERM MHFAMHX MHCONF $1024;
  infile "/app/input/mh_supp_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. MHTERM : $char1024. MHFAMHX : $char1024. MHCONF : $char1024.;
run;

proc sql; create table qualified as select a.*,b.MHSEQ from mh_supp_raw a inner join mh b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.MHTERM=b.MHTERM; quit;
data result; set qualified; length RDOMAIN IDVAR IDVARVAL QNAM QLABEL QVAL QORIG QEVAL $1024; RDOMAIN='MH'; IDVAR='MHSEQ'; IDVARVAL=strip(put(MHSEQ,best32.)); QORIG='Collected'; QEVAL='';
if not missing(MHFAMHX) then do; QNAM='MHFAMHX'; QLABEL='Family History'; QVAL=MHFAMHX; output; end;
if not missing(MHCONF) then do; QNAM='MHCONF'; QLABEL='Confirmed by Medical Records'; QVAL=MHCONF; output; end; run;
proc sql;
  create table final as select STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL from result;
quit;
proc export data=final outfile="/app/output/suppmh.csv" dbms=csv replace; run;
