/* SAS language reference solution for the yamaa benchmark adam-adqs-missed-visit-locf.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID EFFFL $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EFFFL : $char1024.;
run;

data qs;
  length STUDYID USUBJID PARAMCD AVISIT $1024;
  infile "/app/input/qs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. QSSEQ PARAMCD : $char1024. AVISIT : $char1024. AVISITN AVAL;
run;

proc sql;
  create table collected as select a.STUDYID,a.USUBJID,a.PARAMCD,a.AVISIT,a.AVISITN,a.AVAL,'' as DTYPE,b.EFFFL from qs as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where not missing(a.AVAL);
quit;
data schedule; length AVISIT PARAMCD $1024; set adsl; if EFFFL='Y'; PARAMCD='ACTOT'; do AVISITN=8,16,24; AVISIT=catx(' ','Week',strip(put(AVISITN,best32.))); output; end; run;
proc sql;
  create table missed as select a.* from schedule as a left join collected as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARAMCD=b.PARAMCD and a.AVISITN=b.AVISITN where missing(b.AVISITN);
  create table candidates as select a.*,b.AVAL,b.AVISITN as sourcevisit from missed as a left join collected as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PARAMCD=b.PARAMCD and b.AVISITN<a.AVISITN;
quit;
proc sort data=candidates; by STUDYID USUBJID PARAMCD AVISITN descending sourcevisit; run;
data carried; length DTYPE $11; set candidates; by STUDYID USUBJID PARAMCD AVISITN; if first.AVISITN; DTYPE='LOCF'; run;
data result; length DTYPE $11; set collected carried; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL from result;
quit;
proc export data=final outfile="/app/output/adqs.csv" dbms=csv replace; run;
