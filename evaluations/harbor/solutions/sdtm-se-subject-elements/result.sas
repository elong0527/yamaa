/* opensas reference solution for the yamaa benchmark sdtm-se-subject-elements.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID ARMCD RFICDTC RFSTDTC RFPENDTC $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ARMCD : $char1024. RFICDTC : $char1024. RFSTDTC : $char1024. RFPENDTC : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID; run;
data elements;
  length ELEMENT ORD EPOCH $1024;
  retain ELEMENT ORD EPOCH;
  set ordered;

  by StudyOID SubjectKey StudyEventOID;
  if first.StudyEventOID then do;
    ELEMENT=''; ORD=''; EPOCH='';
  end;
    if ItemOID='IT.TE.ELEMENT' then ELEMENT=Value;
  if ItemOID='IT.TA.TAETORD' then ORD=Value;
  if ItemOID='IT.TA.EPOCH' then EPOCH=Value;
  if last.StudyEventOID then output;
run;
proc sort data=odm out=ordered; by StudyOID SubjectKey; run;
data dosing;
  length FIRSTDTC LASTDTC $1024;
  retain FIRSTDTC LASTDTC;
  set ordered;

  by StudyOID SubjectKey;
  if first.SubjectKey then do;
    FIRSTDTC=''; LASTDTC='';
  end;
    if ItemOID='IT.EX.FIRSTDTC' then FIRSTDTC=Value;
  if ItemOID='IT.EX.LASTDTC' then LASTDTC=Value;
  if last.SubjectKey then output;
run;
proc sql; create table joined as select a.*,b.FIRSTDTC,b.LASTDTC,c.RFICDTC,c.RFSTDTC,c.RFPENDTC from elements a left join dosing b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey inner join dm c on a.StudyOID=c.STUDYID and a.SubjectKey=c.USUBJID where not missing(a.ELEMENT); quit;
data result; set joined; length DOMAIN $2 STUDYID USUBJID ETCD SESTDTC SEENDTC SEUPDES $1024; DOMAIN='SE'; STUDYID=StudyOID; USUBJID=SubjectKey; ETCD=StudyEventOID; TAETORD=input(ORD,best32.); SESEQ=TAETORD; SEUPDES=''; SESTDY=.; SEENDY=.;
if EPOCH='SCREENING' then do; SESTDTC=RFICDTC; SEENDTC=FIRSTDTC; end; else if EPOCH='TREATMENT' then do; SESTDTC=FIRSTDTC; SEENDTC=LASTDTC; end; else do; SESTDTC=LASTDTC; SEENDTC=RFPENDTC; end;
if not missing(RFSTDTC) then do; if not missing(SESTDTC) then do; SESTDY=input(SESTDTC,yymmdd10.)-input(RFSTDTC,yymmdd10.); if SESTDY>=0 then SESTDY+1; end; if not missing(SEENDTC) then do; SEENDY=input(SEENDTC,yymmdd10.)-input(RFSTDTC,yymmdd10.); if SEENDY>=0 then SEENDY+1; end; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, SESEQ, ETCD, ELEMENT, TAETORD, EPOCH, SESTDTC, SEENDTC, SESTDY, SEENDY, SEUPDES from result;
quit;
proc export data=final outfile="/app/output/se.csv" dbms=csv replace; run;
