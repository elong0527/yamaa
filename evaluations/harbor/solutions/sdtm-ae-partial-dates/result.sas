/* SAS language reference solution for the yamaa benchmark sdtm-ae-partial-dates.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID RFSTDTC $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RFSTDTC : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. FormOID : $char1024. FormRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey FormRepeatKey; run;
data wide;
  length AETERM AESTYR AESTMO AEENYR AEENMO STARTDAY ENDDAY $1024;
  retain AETERM AESTYR AESTMO AEENYR AEENMO STARTDAY ENDDAY;
  set ordered;
  where ItemGroupOID='IG.AE';
  by StudyOID SubjectKey FormRepeatKey;
  if first.FormRepeatKey then do;
    AETERM=''; AESTYR=''; AESTMO=''; AEENYR=''; AEENMO=''; STARTDAY=''; ENDDAY='';
  end;
    if ItemOID='IT.AE.AETERM' then AETERM=Value;
  if ItemOID='IT.AE.AESTYR' then AESTYR=Value;
  if ItemOID='IT.AE.AESTMO' then AESTMO=Value;
  if ItemOID='IT.AE.AEENYR' then AEENYR=Value;
  if ItemOID='IT.AE.AEENMO' then AEENMO=Value;
  if ItemOID='IT.AE.AESTDY' then STARTDAY=Value;
  if ItemOID='IT.AE.AEENDY' then ENDDAY=Value;
  if last.FormRepeatKey then output;
run;
data dates; set wide; length DOMAIN $2 STUDYID USUBJID AESTDTC AEENDTC $1024;
DOMAIN='AE'; STUDYID=StudyOID; USUBJID=SubjectKey; AESEQ=input(FormRepeatKey,best32.);
AESTDTC=''; AEENDTC='';
if not missing(AESTYR) then do; AESTDTC=AESTYR; if not missing(AESTMO) then do; AESTDTC=catx('-',AESTDTC,AESTMO); if not missing(STARTDAY) then AESTDTC=catx('-',AESTDTC,STARTDAY); end; end;
if not missing(AEENYR) then do; AEENDTC=AEENYR; if not missing(AEENMO) then do; AEENDTC=catx('-',AEENDTC,AEENMO); if not missing(ENDDAY) then AEENDTC=catx('-',AEENDTC,ENDDAY); end; end; run;
proc sql; create table joined as select a.*,b.RFSTDTC from dates a left join dm b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data result; set joined; AESTDY=.; AEENDY=.; if lengthn(RFSTDTC)=10 then do;
if lengthn(AESTDTC)=10 then do; AESTDY=input(AESTDTC,yymmdd10.)-input(RFSTDTC,yymmdd10.); if AESTDY>=0 then AESTDY+1; end;
if lengthn(AEENDTC)=10 then do; AEENDY=input(AEENDTC,yymmdd10.)-input(RFSTDTC,yymmdd10.); if AEENDY>=0 then AEENDY+1; end; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESTDY, AEENDY from result;
quit;
proc export data=final outfile="/app/output/ae.csv" dbms=csv replace; run;
