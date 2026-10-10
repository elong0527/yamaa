/* SAS language reference solution for the yamaa benchmark sdtm-ds-reasons.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID; run;
data wide;
  length COMP DTC REASON REASONCD $1024;
  retain COMP DTC REASON REASONCD;
  set ordered;

  by StudyOID SubjectKey StudyEventOID;
  if first.StudyEventOID then do;
    COMP=''; DTC=''; REASON=''; REASONCD='';
  end;
    if ItemOID='IT.DS.COMP' then COMP=Value;
  if ItemOID='IT.DS.DTC' then DTC=Value;
  if ItemOID='IT.DS.REASON' then REASON=Value;
  if ItemOID='IT.DS.REASONCD' then REASONCD=Value;
  if last.StudyEventOID then output;
run;
data prepared; set wide; length DOMAIN $2 STUDYID USUBJID DSTERM DSDECOD DSCAT DSSCAT DSSTDTC $1024;
DOMAIN='DS'; STUDYID=StudyOID; USUBJID=SubjectKey; DSCAT='DISPOSITION EVENT'; DSSTDTC=DTC; DSTERM=REASON; DSDECOD=REASONCD;
if COMP='COMPLETED' then do; DSTERM='COMPLETED'; DSDECOD='COMPLETED'; end;
if StudyEventOID='EOT' then do; DSSEQ=1; DSSCAT='STUDY TREATMENT'; end; else do; DSSEQ=2; DSSCAT='STUDY'; end; run; data result; set prepared; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, DSSEQ, DSCAT, DSSCAT, DSTERM, DSDECOD, DSSTDTC from result;
quit;
proc export data=final outfile="/app/output/ds.csv" dbms=csv replace; run;
