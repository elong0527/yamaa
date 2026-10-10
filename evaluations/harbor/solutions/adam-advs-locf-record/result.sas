/* opensas reference solution for the yamaa benchmark adam-advs-locf-record.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data plan;
  length USUBJID PARAMCD $1024;
  infile "/app/input/plan.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input USUBJID : $char1024. PARAMCD : $char1024. AVISITN;
run;

data vs;
  length USUBJID PARAMCD ADT ANL01FL $1024;
  infile "/app/input/vs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input USUBJID : $char1024. PARAMCD : $char1024. AVISITN AVAL ADT : $char1024. QSSEQ ANL01FL : $char1024.;
run;

proc sql;
  create table candidates as select a.USUBJID,a.PARAMCD,a.AVISITN,b.AVAL,b.ADT,b.QSSEQ,b.AVISITN as observedvisit from plan as a left join vs as b
    on a.USUBJID=b.USUBJID and a.PARAMCD=b.PARAMCD and b.AVISITN<=a.AVISITN and b.ANL01FL='Y' and not missing(b.AVAL);
quit;
proc sort data=candidates; by USUBJID PARAMCD AVISITN descending observedvisit descending QSSEQ; run;
data result; set candidates; by USUBJID PARAMCD AVISITN; if first.AVISITN; run;

proc sql;
  create table final as select USUBJID, PARAMCD, AVISITN, AVAL, ADT, QSSEQ from result;
quit;
proc export data=final outfile="/app/output/advs.csv" dbms=csv replace; run;
