/* opensas reference solution for the yamaa benchmark sdtm-lb-metadata.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey; run;
data dates;
  length LBDTC $1024;
  retain LBDTC;
  set ordered;

  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    LBDTC='';
  end;
    if ItemOID='IT.LB.LBDTC' then LBDTC=Value;
  if last.ItemGroupRepeatKey then output;
run;
proc sql;
create table joined as select a.*,b.LBDTC from odm a inner join dates b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey and a.ItemGroupRepeatKey=b.ItemGroupRepeatKey where a.ItemOID in ('IT.LB.GLUC','IT.LB.CREAT') and not missing(a.Value); quit;
data prepared; set joined; length DOMAIN $2 STUDYID USUBJID LBTESTCD LBTEST LBORRES LBORRESU LBSTRESU LBSTAT $1024;
DOMAIN='LB'; STUDYID=StudyOID; USUBJID=SubjectKey; LBORRES=Value; LBORRESU='mg/dL'; LBSTRESU='mg/dL'; LBSTRESN=input(Value,?? best32.); LBSTAT='';
if ItemOID='IT.LB.CREAT' then do; LBTESTCD='CREAT'; LBTEST='Creatinine'; end; else do; LBTESTCD='GLUC'; LBTEST='Glucose'; end;
if Value='NOT DONE' then do; LBORRES=''; LBORRESU=''; LBSTRESU=''; LBSTRESN=.; LBSTAT='NOT DONE'; end; run;
proc sort data=prepared; by STUDYID USUBJID LBDTC LBTESTCD; run;
data result; set prepared; by STUDYID USUBJID; retain LBSEQ;
  if first.USUBJID then LBSEQ=0;
  LBSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU, LBSTRESN, LBSTRESU, LBDTC from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
