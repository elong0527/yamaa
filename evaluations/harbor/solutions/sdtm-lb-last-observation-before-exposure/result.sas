/* opensas reference solution for the yamaa benchmark sdtm-lb-last-observation-before-exposure.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID RFSTDTC $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RFSTDTC : $char1024.;
run;

data lb_mapping;
  length ItemOID LBTESTCD LBSPEC $1024;
  infile "/app/input/lb_mapping.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input ItemOID : $char1024. LBTESTCD : $char1024. LBSPEC : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. FormOID : $char1024. FormRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupRepeatKey; run;
data dates;
  length LBDTC LBSTAT $1024;
  retain LBDTC LBSTAT;
  set ordered;

  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    LBDTC=''; LBSTAT='';
  end;
    if ItemOID='IT.LB.LBDTC' then LBDTC=Value;
  if ItemOID='IT.LB.LBSTAT' then LBSTAT=Value;
  if last.ItemGroupRepeatKey then output;
run;
proc sql;
create table joined as select a.*,b.LBDTC,b.LBSTAT,c.LBTESTCD,c.LBSPEC,d.RFSTDTC from odm a inner join dates b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey and a.FormOID=b.FormOID and a.FormRepeatKey=b.FormRepeatKey and a.ItemGroupRepeatKey=b.ItemGroupRepeatKey inner join lb_mapping c on a.ItemOID=c.ItemOID left join dm d on a.StudyOID=d.STUDYID and a.SubjectKey=d.USUBJID; quit;
data collected; set joined; length DOMAIN $2 STUDYID USUBJID LBORRES DAYDATE REFDAY $1024; DOMAIN='LB'; STUDYID=StudyOID; USUBJID=SubjectKey; LBSEQ=input(ItemGroupRepeatKey,best32.); LBORRES=Value;
if missing(Value) then LBSTAT='NOT DONE'; if LBSTAT='NOT DONE' then LBORRES=''; DAYDATE=''; REFDAY=''; if lengthn(LBDTC)>=10 then DAYDATE=substr(LBDTC,1,10); if lengthn(RFSTDTC)>=10 then REFDAY=substr(RFSTDTC,1,10); run;
data eligible; set collected; if not missing(LBORRES) and not missing(DAYDATE) and (missing(REFDAY) or DAYDATE<=REFDAY); run;
proc sort data=eligible; by STUDYID USUBJID LBTESTCD LBSPEC descending DAYDATE descending LBSEQ; run;
data flagged; set eligible; by STUDYID USUBJID LBTESTCD LBSPEC; if first.LBSPEC; length LBLOBXFL $1; LBLOBXFL='Y'; run;
proc sql; create table result as select a.*,b.LBLOBXFL from collected a left join flagged b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.LBSEQ=b.LBSEQ and a.LBTESTCD=b.LBTESTCD and a.LBSPEC=b.LBSPEC; quit;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSPEC, LBORRES, LBDTC, LBSTAT, LBLOBXFL from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
