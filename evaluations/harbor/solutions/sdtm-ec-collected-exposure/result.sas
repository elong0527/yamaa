/* opensas reference solution for the yamaa benchmark sdtm-ec-collected-exposure.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dosing_log;
  length STUDYID USUBJID LOGDATE TABLETS DOSE_TAKEN MISSREAS DOSE_ADJ $1024;
  infile "/app/input/dosing_log.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LOGDATE : $char1024. TABLETS : $char1024. DOSE_TAKEN : $char1024. MISSREAS : $char1024. DOSE_ADJ : $char1024.;
run;

data prepared; set dosing_log; length DOMAIN $2 ECTRT ECMOOD ECOCCUR ECREASND ECDOSU ECDOSFRM ECDOSFRQ ECROUTE ECSTDTC ECENDTC ECADJ $1024;
DOMAIN='EC'; ECTRT='Study Drug'; ECMOOD='PERFORMED'; ECOCCUR=DOSE_TAKEN; ECDOSE=.; ECREASND='';
if ECOCCUR='Y' then ECDOSE=input(TABLETS,best32.); else ECREASND=MISSREAS;
ECDOSU='tablet'; ECDOSFRM='TABLET'; ECDOSFRQ='QD'; ECROUTE='ORAL'; ECSTDTC=LOGDATE; ECENDTC=LOGDATE; ECADJ=DOSE_ADJ; run;
proc sort data=prepared; by STUDYID USUBJID ECSTDTC; run;
data result; set prepared; by STUDYID USUBJID; retain ECSEQ;
  if first.USUBJID then ECSEQ=0;
  ECSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, ECSEQ, ECTRT, ECMOOD, ECOCCUR, ECREASND, ECDOSE, ECDOSU, ECDOSFRM, ECDOSFRQ, ECROUTE, ECSTDTC, ECENDTC, ECADJ from result;
quit;
proc export data=final outfile="/app/output/ec.csv" dbms=csv replace; run;
