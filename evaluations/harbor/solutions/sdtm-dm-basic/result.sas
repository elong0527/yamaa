/* opensas reference solution for the yamaa benchmark sdtm-dm-basic.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey; run;
data wide;
  length SITEID SEX AGE ARM COUNTRY $1024;
  retain SITEID SEX AGE ARM COUNTRY;
  set ordered;
  by StudyOID SubjectKey;
  if first.SubjectKey then do;
    SITEID=''; SEX=''; AGE=''; ARM=''; COUNTRY='';
  end;
    if ItemOID='IT.DM.SEX' then SEX=Value;
  if ItemOID='IT.DM.AGE' then AGE=Value;
  if ItemOID='IT.DM.ARM' then ARM=Value;
  if ItemOID='IT.DM.SITEID' then SITEID=Value;
  if ItemOID='IT.DM.COUNTRY' then COUNTRY=Value;
  if last.SubjectKey then output;
run;
data result; set wide; length DOMAIN $2 STUDYID USUBJID SUBJID AGEU ACTARM ARMNRS $1024;
DOMAIN='DM'; STUDYID=StudyOID; USUBJID=SubjectKey; SUBJID=SubjectKey;
if SEX='Male' then SEX='M'; else if SEX='Female' then SEX='F'; else SEX='U'; if not missing(AGE) then AGEU='YEARS'; ACTARM=ARM; if missing(ARM) then ARMNRS='NOT ASSIGNED';
run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, ARM, ACTARM, ARMNRS, COUNTRY from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
