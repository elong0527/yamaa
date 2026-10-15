/* opensas reference solution for the yamaa benchmark adam-adpp-dose-normalized.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data pp;
  length STUDYID DOMAIN USUBJID PPSEQ PPTESTCD PPTEST PPCAT PPORRES PPORRESU PPSTRESC PPSTRESU PPSTAT PPREASND PPSPEC VISITNUM VISIT PPRFTDTC $1024;
  infile "/app/input/pp.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. PPSEQ : $char1024. PPTESTCD : $char1024. PPTEST : $char1024. PPCAT : $char1024. PPORRES : $char1024. PPORRESU : $char1024. PPSTRESC : $char1024. PPSTRESN PPSTRESU : $char1024. PPSTAT : $char1024. PPREASND : $char1024. PPSPEC : $char1024. VISITNUM : $char1024. VISIT : $char1024. PPRFTDTC : $char1024.;
run;

data ex;
  length STUDYID DOMAIN USUBJID EXSEQ EXTRT EXDOSU EXDOSFRM EXDOSFRQ EXROUTE EXSTDTC EXENDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. EXSEQ : $char1024. EXTRT : $char1024. EXDOSE EXDOSU : $char1024. EXDOSFRM : $char1024. EXDOSFRQ : $char1024. EXROUTE : $char1024. EXSTDTC : $char1024. EXENDTC : $char1024.;
run;

/* Every PP record as collected. */
data kept;
  length STUDYID USUBJID VISIT PARAMCD PARAM AVALU $1024;
  set pp;
  PARAMCD=PPTESTCD; PARAM=PPTEST; AVAL=PPSTRESN; AVALU=PPSTRESU;
  keep STUDYID USUBJID VISIT PARAMCD PARAM AVAL AVALU;
run;

/* The dose in mg of each administration; a dose in ug is a thousandth of a mg. */
data dose;
  length PPRFTDTC $1024;
  set ex;
  if EXDOSU='mg' then DOSEMG=EXDOSE;
  else if EXDOSU='ug' then DOSEMG=EXDOSE/1000;
  PPRFTDTC=EXSTDTC;
  keep STUDYID USUBJID PPRFTDTC DOSEMG;
run;

/* The maximum concentration and area under the curve records that have a value. */
data exposure;
  length NEWCD NEWPARAM $1024;
  set pp;
  if PPTESTCD='CMAX' then do; NEWCD='CMAXD'; NEWPARAM='Max Conc Norm by Dose'; end;
  else if PPTESTCD='AUCLST' then do; NEWCD='AUCLSTD'; NEWPARAM='AUC to Last Nonzero Conc Norm by Dose'; end;
  else if PPTESTCD='AUCIFO' then do; NEWCD='AUCIFOD'; NEWPARAM='AUC Infinity Obs Norm by Dose'; end;
  if NEWCD ne '' and not missing(PPSTRESN);
run;

/* The administration that starts at the profile's reference time. */
proc sql;
  create table matched as select a.*, b.DOSEMG from exposure as a inner join dose as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.PPRFTDTC=b.PPRFTDTC;
quit;

data added;
  length STUDYID USUBJID VISIT PARAMCD PARAM AVALU $1024;
  set matched;
  if not missing(DOSEMG) and DOSEMG ne 0;
  PARAMCD=NEWCD; PARAM=NEWPARAM; AVAL=PPSTRESN/DOSEMG; AVALU=cats(PPSTRESU, '/mg');
  keep STUDYID USUBJID VISIT PARAMCD PARAM AVAL AVALU;
run;

data result; set kept added; run;

proc sql;
  create table final as select STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AVALU from result;
quit;
proc export data=final outfile="/app/output/adpp.csv" dbms=csv replace; run;
