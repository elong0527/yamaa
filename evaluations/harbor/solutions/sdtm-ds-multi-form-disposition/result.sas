/* opensas reference solution for the yamaa benchmark sdtm-ds-multi-form-disposition.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

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
USUBJID=catx('-',StudyOID,SubjectKey);
if StudyEventOID='CONSENT' then do; DSTERM='INFORMED CONSENT OBTAINED'; DSDECOD=DSTERM; DSCAT='PROTOCOL MILESTONE'; DSSCAT='INFORMED CONSENT'; end;
if StudyEventOID='RAND' then do; DSTERM='RANDOMIZED'; DSDECOD=DSTERM; DSCAT='PROTOCOL MILESTONE'; DSSCAT='RANDOMIZATION'; end;
if StudyEventOID='EOT' then DSSCAT='END OF TREATMENT';
if StudyEventOID='EOS' then do; DSSCAT='END OF STUDY'; if COMP='SCREEN FAILURE' then do; DSTERM='SCREEN FAILURE'; DSDECOD=DSTERM; end; end; run;
proc sort data=prepared; by STUDYID USUBJID DSSTDTC DSTERM; run;
data result; set prepared; by STUDYID USUBJID; retain DSSEQ;
  if first.USUBJID then DSSEQ=0;
  DSSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, DSSEQ, DSTERM, DSDECOD, DSCAT, DSSCAT, DSSTDTC from result;
quit;
proc export data=final outfile="/app/output/ds.csv" dbms=csv replace; run;
