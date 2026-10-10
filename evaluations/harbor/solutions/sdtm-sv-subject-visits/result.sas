/* SAS language reference solution for the yamaa benchmark sdtm-sv-subject-visits.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID RFSTDTC $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RFSTDTC : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey; run;
data visits;
  length VISIT VNUM VDAY ORD EPOCH SVUPDES $1024;
  retain VISIT VNUM VDAY ORD EPOCH SVUPDES;
  set ordered;

  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey;
  if first.StudyEventRepeatKey then do;
    VISIT=''; VNUM=''; VDAY=''; ORD=''; EPOCH=''; SVUPDES='';
  end;
    if ItemOID='IT.TV.VISIT' then VISIT=Value;
  if ItemOID='IT.TV.VISITNUM' then VNUM=Value;
  if ItemOID='IT.TV.VISITDY' then VDAY=Value;
  if ItemOID='IT.TV.TAETORD' then ORD=Value;
  if ItemOID='IT.TV.EPOCH' then EPOCH=Value;
  if ItemOID='IT.TV.UPDES' then SVUPDES=Value;
  if last.StudyEventRepeatKey then output;
run;
proc sql;
create table dates as select StudyOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,min(Value) as SVSTDTC,max(Value) as SVENDTC from odm where ItemOID in ('IT.VS.VSDTC','IT.LB.LBDTC','IT.EX.EXDTC') and not missing(Value) group by StudyOID,SubjectKey,StudyEventOID,StudyEventRepeatKey;
create table joined as select a.*,b.SVSTDTC,b.SVENDTC,c.RFSTDTC from visits a inner join dates b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey left join dm c on a.StudyOID=c.STUDYID and a.SubjectKey=c.USUBJID; quit;
data prepared; set joined; length DOMAIN $2 STUDYID USUBJID $1024; DOMAIN='SV'; STUDYID=StudyOID; USUBJID=SubjectKey; VISITNUM=input(VNUM,best32.); VISITDY=input(VDAY,best32.); TAETORD=input(ORD,best32.); SVSTDY=.; SVENDY=.; if not missing(RFSTDTC) then do; SVSTDY=input(SVSTDTC,yymmdd10.)-input(RFSTDTC,yymmdd10.); if SVSTDY>=0 then SVSTDY+1; SVENDY=input(SVENDTC,yymmdd10.)-input(RFSTDTC,yymmdd10.); if SVENDY>=0 then SVENDY+1; end; run;
proc sort data=prepared; by STUDYID USUBJID SVSTDTC StudyEventOID StudyEventRepeatKey; run;
data result; set prepared; by STUDYID USUBJID; retain SVSEQ UNSCH;
if first.USUBJID then do; SVSEQ=0; UNSCH=0; end; SVSEQ+1;
if StudyEventOID='UNSCH' then do; UNSCH+1; VISITNUM=99+UNSCH/10; VISITDY=.; TAETORD=.; end; else SVUPDES=''; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, SVSEQ, VISIT, VISITNUM, VISITDY, SVSTDTC, SVENDTC, SVSTDY, SVENDY, TAETORD, EPOCH, SVUPDES from result;
quit;
proc export data=final outfile="/app/output/sv.csv" dbms=csv replace; run;
