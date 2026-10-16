/* opensas reference solution for the yamaa benchmark adam-adeg-qtc-flags.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adeg;
  length STUDYID USUBJID PARAMCD PARAM AVISIT ABLFL ONTRTFL $1024;
  infile "/app/input/adeg_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. PARAM : $char1024. AVISIT : $char1024. AVAL ABLFL : $char1024. ONTRTFL : $char1024. BASE CHG;
run;

data result;
  length CRIT1 CRIT2 CRIT3 CRIT4 CRIT5 $1024 CRIT1FL CRIT2FL CRIT3FL CRIT4FL CRIT5FL $1;
  set adeg;
  CRIT1=''; CRIT1FL=''; CRIT2=''; CRIT2FL=''; CRIT3=''; CRIT3FL='';
  CRIT4=''; CRIT4FL=''; CRIT5=''; CRIT5FL='';
  /* Only on-treatment QTcF records are assessed, and values are compared
     unrounded, strictly above each limit: exactly at a limit reads N. */
  if PARAMCD='QTCF' and ONTRTFL='Y' and not missing(AVAL) then do;
    CRIT1='QTcF > 450 ms'; CRIT1FL=ifc(AVAL>450,'Y','N');
    CRIT2='QTcF > 480 ms'; CRIT2FL=ifc(AVAL>480,'Y','N');
    CRIT3='QTcF > 500 ms'; CRIT3FL=ifc(AVAL>500,'Y','N');
  end;
  if PARAMCD='QTCF' and ONTRTFL='Y' and not missing(CHG) then do;
    CRIT4='QTcF increase from baseline > 30 ms'; CRIT4FL=ifc(CHG>30,'Y','N');
    CRIT5='QTcF increase from baseline > 60 ms'; CRIT5FL=ifc(CHG>60,'Y','N');
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, ABLFL, ONTRTFL, BASE, CHG, CRIT1, CRIT1FL, CRIT2, CRIT2FL, CRIT3, CRIT3FL, CRIT4, CRIT4FL, CRIT5, CRIT5FL from result;
quit;
proc export data=final outfile="/app/output/adeg.csv" dbms=csv replace; run;
