/* opensas reference solution for the yamaa benchmark sdtm-eg-ecg-findings.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. FormOID : $char1024. FormRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

/* One replicate ECG is one repeat of the ECG item group on a time point's
   form: collect its date and time, QT interval and interpretation. */
proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID FormOID ItemGroupRepeatKey; run;
data wide;
  length EGDTC QT INTP $1024;
  retain EGDTC QT INTP;
  set ordered;
  where FormOID in ('FO.EG_PREDOSE','FO.EG_2H');
  by StudyOID SubjectKey StudyEventOID FormOID ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    EGDTC=''; QT=''; INTP='';
  end;
  if ItemOID='IT.EG.EGDTC' then EGDTC=Value;
  if ItemOID='IT.EG.QT' then QT=Value;
  if ItemOID='IT.EG.INTP' then INTP=Value;
  if last.ItemGroupRepeatKey then output;
run;

/* One record per test; a blank result gives no record. */
data prepared; set wide;
  length DOMAIN $2 STUDYID USUBJID EGTESTCD EGTEST EGORRES EGORRESU EGSTRESC EGSTRESU VISIT EGTPT $1024;
  DOMAIN='EG'; STUDYID=StudyOID; USUBJID=SubjectKey;
  VISIT=''; if StudyEventOID='SE.DAY1' then VISIT='DAY 1';
  EGTPT=''; EGTPTNUM=.;
  if FormOID='FO.EG_PREDOSE' then do; EGTPT='PRE-DOSE'; EGTPTNUM=1; end;
  if FormOID='FO.EG_2H' then do; EGTPT='2 H POST-DOSE'; EGTPTNUM=2; end;
  EGREPNUM=input(ItemGroupRepeatKey,best32.);
  if not missing(QT) then do;
    EGTESTCD='QTAG'; EGTEST='QT Interval, Aggregate';
    EGORRES=strip(QT); EGORRESU='msec'; EGSTRESC=EGORRES;
    EGSTRESN=input(EGORRES,best32.); EGSTRESU='msec';
    output;
  end;
  if not missing(INTP) then do;
    EGTESTCD='INTP'; EGTEST='Interpretation';
    EGORRES=strip(INTP); EGORRESU=''; EGSTRESC=EGORRES;
    EGSTRESN=.; EGSTRESU='';
    output;
  end;
run;
proc sort data=prepared; by STUDYID USUBJID EGTPTNUM EGREPNUM EGTESTCD; run;
data result; set prepared; by STUDYID USUBJID; retain EGSEQ;
  if first.USUBJID then EGSEQ=0;
  EGSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, EGSEQ, EGTESTCD, EGTEST, EGORRES, EGORRESU, EGSTRESC, EGSTRESN, EGSTRESU, EGREPNUM, VISIT, EGDTC, EGTPT, EGTPTNUM from result;
quit;
proc export data=final outfile="/app/output/eg.csv" dbms=csv replace; run;
