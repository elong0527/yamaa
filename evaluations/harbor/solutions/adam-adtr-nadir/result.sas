/* SAS language reference solution for the yamaa benchmark adam-adtr-nadir.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adtr_raw;
  length STUDYID USUBJID AVISIT ADT PARAMCD PARAM ANL01FL $1024;
  infile "/app/input/adtr_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AVISIT : $char1024. AVISITN ADT : $char1024. PARAMCD : $char1024. PARAM : $char1024. AVAL NMEAS NTARGET ANL01FL : $char1024.;
run;

data history; set adtr_raw; PRIORDATE=ADT; PRIORVALUE=AVAL; PRIORFLAG=ANL01FL; keep STUDYID USUBJID PRIORDATE PRIORVALUE PRIORFLAG; run;
proc sql;
create table nadirs as select a.STUDYID,a.USUBJID,a.AVISIT,min(b.PRIORVALUE) as NADIR from adtr_raw a left join history b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and not missing(a.ADT) and not missing(b.PRIORDATE) and b.PRIORDATE<=a.ADT and b.PRIORFLAG='Y' group by a.STUDYID,a.USUBJID,a.AVISIT;
create table result as select a.*,b.NADIR from adtr_raw a left join nadirs b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AVISIT=b.AVISIT; quit;
proc sql;
  create table final as select STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS, NTARGET, ANL01FL, NADIR from result;
quit;
proc export data=final outfile="/app/output/adtr.csv" dbms=csv replace; run;
