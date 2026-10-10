/* SAS language reference solution for the yamaa benchmark sdtm-fa-event-findings.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey; run;
data events;
  length FAOBJ AELNKID $1024;
  retain FAOBJ AELNKID;
  set ordered;
  where ItemGroupOID='IG.AE';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    FAOBJ=''; AELNKID='';
  end;
    if ItemOID='IT.AE.AETERM' then FAOBJ=Value;
  if ItemOID='IT.AE.AELNKID' then AELNKID=Value;
  if last.ItemGroupRepeatKey then output;
run;
proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey; run;
data findings;
  length FALNKID LOC SIZE SIZEU BIOPSY FADTC $1024;
  retain FALNKID LOC SIZE SIZEU BIOPSY FADTC;
  set ordered;
  where ItemGroupOID='IG.FA';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    FALNKID=''; LOC=''; SIZE=''; SIZEU=''; BIOPSY=''; FADTC='';
  end;
    if ItemOID='IT.FA.AELNKID' then FALNKID=Value;
  if ItemOID='IT.FA.LOCATION' then LOC=Value;
  if ItemOID='IT.FA.SIZE' then SIZE=Value;
  if ItemOID='IT.FA.SIZEU' then SIZEU=Value;
  if ItemOID='IT.FA.BIOPSY' then BIOPSY=Value;
  if ItemOID='IT.FA.FADTC' then FADTC=Value;
  if last.ItemGroupRepeatKey then output;
run;
proc sql;
create table joined as select a.*,b.FAOBJ from findings a inner join events b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.FALNKID=b.AELNKID; quit;
data prepared; set joined; length DOMAIN $2 STUDYID USUBJID FATESTCD FATEST FACAT FAORRES FAORRESU FASTRESC FASTRESU $1024;
DOMAIN='FA'; STUDYID=StudyOID; USUBJID=SubjectKey; FACAT='AE'; FAORRESU=''; FASTRESU=''; FASTRESN=.; EVENTREP=input(ItemGroupRepeatKey,best32.);
if not missing(LOC) then do; FATESTCD='LOC'; FATEST='Location'; FAORRES=LOC; FASTRESC=FAORRES; TESTORD=1; output; end;
if not missing(SIZE) then do; FATESTCD='SIZE'; FATEST='Size'; FAORRES=SIZE; FASTRESC=FAORRES; FAORRESU=SIZEU; FASTRESU=SIZEU; FASTRESN=input(SIZE,best32.); TESTORD=2; output; end;
if not missing(BIOPSY) then do; FATESTCD='BIOPSY'; FATEST='Biopsied'; FAORRES=BIOPSY; FASTRESC=FAORRES; FAORRESU=''; FASTRESU=''; FASTRESN=.; TESTORD=3; output; end; run;
proc sort data=prepared; by STUDYID USUBJID FALNKID TESTORD FAORRES; run;
data result; set prepared; by STUDYID USUBJID; retain FASEQ;
  if first.USUBJID then FASEQ=0;
  FASEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FALNKID, FADTC from result;
quit;
proc export data=final outfile="/app/output/fa.csv" dbms=csv replace; run;
