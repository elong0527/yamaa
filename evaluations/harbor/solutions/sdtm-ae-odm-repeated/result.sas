/* opensas reference solution for the yamaa benchmark sdtm-ae-odm-repeated.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. FormOID : $char1024. FormRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey; run;
data wide;
  length AETERM AESTDTC AEENDTC AESEV AESER $1024;
  retain AETERM AESTDTC AEENDTC AESEV AESER;
  set ordered;
  where ItemGroupOID='IG.AE';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    AETERM=''; AESTDTC=''; AEENDTC=''; AESEV=''; AESER='';
  end;
    if ItemOID='IT.AE.AETERM' then AETERM=Value;
  if ItemOID='IT.AE.AESTDTC' then AESTDTC=Value;
  if ItemOID='IT.AE.AEENDTC' then AEENDTC=Value;
  if ItemOID='IT.AE.AESEV' then AESEV=Value;
  if ItemOID='IT.AE.AESER' then AESER=Value;
  if last.ItemGroupRepeatKey then output;
run;
data prepared; set wide; if not missing(AETERM);
length DOMAIN $2 STUDYID USUBJID $1024; DOMAIN='AE'; STUDYID=StudyOID; USUBJID=SubjectKey;
VISITORD=2; if StudyEventOID='SCREENING' then VISITORD=0; if StudyEventOID='BASELINE' then VISITORD=1;
VISITREP=input(StudyEventRepeatKey,best32.); IGREP=input(ItemGroupRepeatKey,best32.); run;
proc sort data=prepared; by STUDYID USUBJID VISITORD VISITREP IGREP; run;
data result; set prepared; by STUDYID USUBJID; retain AESEQ;
  if first.USUBJID then AESEQ=0;
  AESEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESEV, AESER from result;
quit;
proc export data=final outfile="/app/output/ae.csv" dbms=csv replace; run;
