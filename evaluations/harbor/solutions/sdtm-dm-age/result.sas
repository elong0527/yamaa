/* SAS language reference solution for the yamaa benchmark sdtm-dm-age.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey; run;
data wide;
  length BRTHDTC RFICDTC $1024;
  retain BRTHDTC RFICDTC;
  set ordered;

  by StudyOID SubjectKey;
  if first.SubjectKey then do;
    BRTHDTC=''; RFICDTC='';
  end;
    if ItemOID='IT.DM.BRTHDT' then BRTHDTC=Value;
  if ItemOID='IT.DM.RFICDTC' then RFICDTC=Value;
  if last.SubjectKey then output;
run;
data result; set wide; length DOMAIN $2 STUDYID USUBJID $1024 AGEU $8; DOMAIN='DM'; STUDYID=StudyOID; USUBJID=SubjectKey;
if lengthn(BRTHDTC)=10 and lengthn(RFICDTC)=10 then do; AGE=intck('year',input(BRTHDTC,yymmdd10.),input(RFICDTC,yymmdd10.),'continuous'); AGEU='YEARS'; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
