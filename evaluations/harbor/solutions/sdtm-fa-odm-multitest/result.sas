/* SAS language reference solution for the yamaa benchmark sdtm-fa-odm-multitest.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey; run;
data siblings;
  length FAOBJ FADTC $1024;
  retain FAOBJ FADTC;
  set ordered;
  where ItemGroupOID='IG.FA';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    FAOBJ=''; FADTC='';
  end;
    if ItemOID='IT.FA.FAOBJ' then FAOBJ=Value;
  if ItemOID='IT.FA.FADTC' then FADTC=Value;
  if last.ItemGroupRepeatKey then output;
run;
proc sql;
create table joined as select a.*,b.FAOBJ,b.FADTC from odm a inner join siblings b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey and a.ItemGroupRepeatKey=b.ItemGroupRepeatKey where a.ItemOID in ('IT.FA.OCCUR','IT.FA.SEV','IT.FA.LDIAM'); quit;
data prepared; set joined; length DOMAIN $2 STUDYID USUBJID FATESTCD FATEST FACAT FAORRES FAORRESU FASTRESC FASTAT FATPT $1024;
DOMAIN='FA'; STUDYID=StudyOID; USUBJID=SubjectKey; FACAT='REACTOGENICITY'; FAORRES=Value; FASTRESC=Value; FAORRESU=''; FASTAT=''; if missing(Value) then FASTAT='NOT DONE';
if ItemOID='IT.FA.OCCUR' then do; FATESTCD='OCCUR'; FATEST='Occurrence Indicator'; TESTORD=1; end;
if ItemOID='IT.FA.SEV' then do; FATESTCD='SEV'; FATEST='Severity/Intensity'; TESTORD=2; end;
if ItemOID='IT.FA.LDIAM' then do; FATESTCD='LDIAM'; FATEST='Longest Diameter'; FAORRESU='mm'; TESTORD=3; end;
FATPT='END DAY 1'; if StudyEventOID='DAY2' then FATPT='END DAY 2'; DAYORD=input(compress(StudyEventOID,,'kd'),best32.); REP=input(ItemGroupRepeatKey,best32.); run;
proc sort data=prepared; by STUDYID USUBJID DAYORD REP TESTORD; run;
data result; set prepared; by STUDYID USUBJID; retain FASEQ;
  if first.USUBJID then FASEQ=0;
  FASEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES, FAORRESU, FASTRESC, FASTAT, FATPT, FADTC from result;
quit;
proc export data=final outfile="/app/output/fa.csv" dbms=csv replace; run;
