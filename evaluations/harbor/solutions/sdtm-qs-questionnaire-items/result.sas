/* SAS language reference solution for the yamaa benchmark sdtm-qs-questionnaire-items.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data items;
  length ItemOID ItemOrder QSTESTCD QSTEST $1024;
  infile "/app/input/items.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input ItemOID : $char1024. ItemOrder : $char1024. QSTESTCD : $char1024. QSTEST : $char1024.;
run;

data notdone;
  length StudyOID SubjectKey StudyEventOID ItemOID Reason $1024;
  infile "/app/input/notdone.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. ItemOID : $char1024. Reason : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

data visits;
  length StudyOID SubjectKey StudyEventOID VisitDate $1024;
  infile "/app/input/visits.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. VISITNUM VisitDate : $char1024.;
run;

proc sql;
create table answered as select a.*,b.QSTESTCD,b.QSTEST,c.VISITNUM,c.VisitDate from odm a inner join items b on a.ItemOID=b.ItemOID inner join visits c on a.StudyOID=c.StudyOID and a.SubjectKey=c.SubjectKey and a.StudyEventOID=c.StudyEventOID;
create table skipped as select a.*,b.QSTESTCD,b.QSTEST,c.VISITNUM,c.VisitDate from notdone a inner join items b on a.ItemOID=b.ItemOID inner join visits c on a.StudyOID=c.StudyOID and a.SubjectKey=c.SubjectKey and a.StudyEventOID=c.StudyEventOID;
create table refused as select a.*,c.VISITNUM,c.VisitDate from notdone a inner join visits c on a.StudyOID=c.StudyOID and a.SubjectKey=c.SubjectKey and a.StudyEventOID=c.StudyEventOID where missing(a.ItemOID);
quit;
data answers; set answered; length QSORRES QSSTRESC QSSTAT QSREASND QSDRVFL $1024; QSSTRESN=input(Value,best32.); QSSTRESC=strip(put(QSSTRESN,best32.)); QSORRES=''; if QSSTRESN=0 then QSORRES='Not at all'; if QSSTRESN=1 then QSORRES='Several days'; if QSSTRESN=2 then QSORRES='More than half the days'; if QSSTRESN=3 then QSORRES='Nearly every day'; QSSTAT=''; QSREASND=''; QSDRVFL=''; run;
proc sql; create table totals as select StudyOID,SubjectKey,StudyEventOID,VISITNUM,VisitDate,sum(QSSTRESN) as QSSTRESN from answers where not missing(QSSTRESN) group by StudyOID,SubjectKey,StudyEventOID,VISITNUM,VisitDate having count(*)=9; quit;
data totalout; set totals; length QSTESTCD QSTEST QSORRES QSSTRESC QSSTAT QSREASND QSDRVFL $1024; QSTESTCD='PHQ9T'; QSTEST='Patient Health Questionnaire 9 item total score'; QSORRES=''; QSSTRESC=strip(put(QSSTRESN,best32.)); QSSTAT=''; QSREASND=''; QSDRVFL='Y'; run;
data skippedout; set skipped; length QSORRES QSSTRESC QSSTAT QSREASND QSDRVFL $1024; QSORRES=''; QSSTRESC=''; QSSTRESN=.; QSSTAT='NOT DONE'; QSREASND=Reason; QSDRVFL=''; run;
data refusedout; set refused; length QSTESTCD QSTEST QSORRES QSSTRESC QSSTAT QSREASND QSDRVFL $1024; QSTESTCD='QSALL'; QSTEST='All Questionnaires'; QSORRES=''; QSSTRESC=''; QSSTRESN=.; QSSTAT='NOT DONE'; QSREASND=Reason; QSDRVFL=''; run;
data prepared; set answers totalout skippedout refusedout; length DOMAIN $2 STUDYID USUBJID QSCAT QSBLFL QSDTC $1024; DOMAIN='QS'; STUDYID=StudyOID; USUBJID=SubjectKey; QSCAT='PHQ-9'; QSDTC=VisitDate; QSBLFL=''; if VISITNUM=1 then QSBLFL='Y'; run;
proc sort data=prepared; by STUDYID USUBJID VISITNUM QSTESTCD; run;
data result; set prepared; by STUDYID USUBJID; retain QSSEQ;
  if first.USUBJID then QSSEQ=0;
  QSSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC, QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC from result;
quit;
proc export data=final outfile="/app/output/qs.csv" dbms=csv replace; run;
