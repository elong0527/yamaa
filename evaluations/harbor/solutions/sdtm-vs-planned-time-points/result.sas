/* opensas reference solution for the yamaa benchmark sdtm-vs-planned-time-points.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

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

data vs_mapping;
  length ItemOID VSTESTCD VSTEST UNIT $1024;
  infile "/app/input/vs_mapping.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input ItemOID : $char1024. VSTESTCD : $char1024. VSTEST : $char1024. UNIT : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey; run;
data dates;
  length VSDTC $1024;
  retain VSDTC;
  set ordered;
  where ItemGroupOID='IG.VS';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey;
  if first.FormRepeatKey then do;
    VSDTC='';
  end;
    if ItemOID='IT.VS.VSDTC' then VSDTC=Value;
  if last.FormRepeatKey then output;
run;
proc sql; create table joined as select a.*,b.VSDTC,c.VSTESTCD,c.UNIT,d.RFSTDTC from odm a inner join dates b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey and a.FormOID=b.FormOID and a.FormRepeatKey=b.FormRepeatKey inner join vs_mapping c on a.ItemOID=c.ItemOID left join dm d on a.StudyOID=d.STUDYID and a.SubjectKey=d.USUBJID; quit;
data prepared; set joined; length DOMAIN $2 STUDYID USUBJID VISIT VSTPT VSELTM VSORRES VSORRESU VSSTRESU $1024; DOMAIN='VS'; STUDYID=StudyOID; USUBJID=SubjectKey; VISIT='DAY 1'; VSTPT=''; VSELTM=''; VSTPTNUM=.;
if FormOID='FO.VS_PREDOSE' then do; VSTPT='PRE-DOSE'; VSTPTNUM=1; VSELTM='-PT15M'; end;
if FormOID='FO.VS_30MIN' then do; VSTPT='30 MIN POST-DOSE'; VSTPTNUM=2; VSELTM='PT30M'; end;
if FormOID='FO.VS_1H' then do; VSTPT='1 H POST-DOSE'; VSTPTNUM=3; VSELTM='PT1H'; end;
if FormOID='FO.VS_4H' then do; VSTPT='4 H POST-DOSE'; VSTPTNUM=4; VSELTM='PT4H'; end;
VSORRES=Value; VSORRESU=UNIT; VSSTRESU=UNIT; VSSTRESN=input(Value,best32.); VSDY=.; if lengthn(VSDTC)>=10 and lengthn(RFSTDTC)>=10 then do; VSDY=input(substr(VSDTC,1,10),yymmdd10.)-input(substr(RFSTDTC,1,10),yymmdd10.); if VSDY>=0 then VSDY+1; end;
TESTORD=3; if VSTESTCD='PULSE' then TESTORD=1; if VSTESTCD='SYSBP' then TESTORD=2; run;
proc sort data=prepared; by STUDYID USUBJID VSTPTNUM TESTORD; run;
data result; set prepared; by STUDYID USUBJID; retain VSSEQ;
  if first.USUBJID then VSSEQ=0;
  VSSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTPT, VSTPTNUM, VSELTM, VSTESTCD, VSORRES, VSORRESU, VSSTRESN, VSSTRESU, VSDTC, VSDY from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
