/* opensas reference solution for the yamaa benchmark sdtm-lb-lab-specific-ranges.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lbrange;
  length LBNAM LBTESTCD SEX EFFSTDT EFFENDT UNIT $1024;
  infile "/app/input/lbrange.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input LBNAM : $char1024. LBTESTCD : $char1024. SEX : $char1024. AGELO AGEHI EFFSTDT : $char1024. EFFENDT : $char1024. UNIT : $char1024. NRLO NRHI;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey; run;
data subjects;
  length SEX BRTHDTC $1024;
  retain SEX BRTHDTC;
  set ordered;
  where ItemGroupOID='IG.DM';
  by StudyOID SubjectKey;
  if first.SubjectKey then do;
    SEX=''; BRTHDTC='';
  end;
    if ItemOID='IT.DM.SEX' then SEX=Value;
  if ItemOID='IT.DM.BRTHDTC' then BRTHDTC=Value;
  if last.SubjectKey then output;
run;
proc sort data=odm out=ordered; by StudyOID SubjectKey ItemGroupRepeatKey; run;
data panels;
  length LBDTC LBNAM LBTESTCD RESULTTEXT $1024;
  retain LBDTC LBNAM LBTESTCD RESULTTEXT;
  set ordered;
  where ItemGroupOID='IG.LB';
  by StudyOID SubjectKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    LBDTC=''; LBNAM=''; LBTESTCD=''; RESULTTEXT='';
  end;
    if ItemOID='IT.LB.LBDTC' then LBDTC=Value;
  if ItemOID='IT.LB.LBNAM' then LBNAM=Value;
  if ItemOID='IT.LB.LBTESTCD' then LBTESTCD=Value;
  if ItemOID='IT.LB.LBSTRESN' then RESULTTEXT=Value;
  if last.ItemGroupRepeatKey then output;
run;
proc sql; create table withbirth as select a.*,b.SEX,b.BRTHDTC from panels a left join subjects b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey; quit;
data collected; set withbirth; AGE=.; if lengthn(BRTHDTC)=10 and lengthn(LBDTC)=10 then AGE=intck('year',input(BRTHDTC,yymmdd10.),input(LBDTC,yymmdd10.),'continuous'); LBSTRESN=input(RESULTTEXT,?? best32.); LBSEQ=input(ItemGroupRepeatKey,best32.); run;
proc sql; create table joined as select a.*,b.UNIT as LBORRESU,b.NRLO as LBSTNRLO,b.NRHI as LBSTNRHI from collected a left join lbrange b on a.LBNAM=b.LBNAM and a.LBTESTCD=b.LBTESTCD and a.SEX=b.SEX and a.AGE>=b.AGELO and a.AGE<b.AGEHI and a.LBDTC>=b.EFFSTDT and (missing(b.EFFENDT) or a.LBDTC<=b.EFFENDT); quit;
data result; set joined; length DOMAIN $2 STUDYID USUBJID $1024 LBNRIND $8; DOMAIN='LB'; STUDYID=StudyOID; USUBJID=SubjectKey; LBNRIND=''; if not missing(LBSTRESN) and not missing(LBSTNRLO) and not missing(LBSTNRHI) then do; if LBSTRESN<LBSTNRLO then LBNRIND='LOW'; else if LBSTRESN>LBSTNRHI then LBNRIND='HIGH'; else LBNRIND='NORMAL'; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, SEX, LBNAM, LBSTRESN, LBORRESU, LBSTNRLO, LBSTNRHI, LBNRIND from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
