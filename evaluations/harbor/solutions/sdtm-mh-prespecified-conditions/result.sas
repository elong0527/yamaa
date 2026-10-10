/* opensas reference solution for the yamaa benchmark sdtm-mh-prespecified-conditions.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data mh_items;
  length ItemOID MHTERM SORTORD $1024;
  infile "/app/input/mh_items.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input ItemOID : $char1024. MHTERM : $char1024. SORTORD : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sql; create table checklist as select a.*,b.MHTERM,b.SORTORD from odm a inner join mh_items b on a.ItemOID=b.ItemOID; quit;
data checklistout; set checklist; length DOMAIN $2 STUDYID USUBJID MHCAT MHPRESP MHOCCUR MHSTAT $1024; DOMAIN='MH'; STUDYID=StudyOID; USUBJID=SubjectKey; MHCAT='DISEASE-SPECIFIC HISTORY'; MHPRESP='Y'; MHOCCUR=Value; MHSTAT=''; if missing(Value) then MHSTAT='NOT DONE'; SECTION=0; ORD=input(SORTORD,best32.); VISITORD=0; REP=0; run;
data volunteered; set odm; where ItemOID='IT.MH.MHTERM' and not missing(Value); length DOMAIN $2 STUDYID USUBJID MHTERM MHCAT MHPRESP MHOCCUR MHSTAT $1024; DOMAIN='MH'; STUDYID=StudyOID; USUBJID=SubjectKey; MHTERM=Value; MHCAT='GENERAL HISTORY'; MHPRESP=''; MHOCCUR=''; MHSTAT=''; SECTION=1; ORD=0; VISITORD=0; if StudyEventOID='BASELINE' then VISITORD=1; REP=input(ItemGroupRepeatKey,best32.); run;
data prepared; set checklistout volunteered; run;
proc sort data=prepared; by STUDYID USUBJID SECTION ORD VISITORD REP; run;
data result; set prepared; by STUDYID USUBJID; retain MHSEQ;
  if first.USUBJID then MHSEQ=0;
  MHSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, MHSEQ, MHTERM, MHCAT, MHPRESP, MHOCCUR, MHSTAT from result;
quit;
proc export data=final outfile="/app/output/mh.csv" dbms=csv replace; run;
