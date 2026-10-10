/* SAS language reference solution for the yamaa benchmark sdtm-vs-replicates.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

data readings; set odm; where ItemOID in ('IT.VS.SYSBP','IT.VS.DIABP') and not missing(Value); length STUDYID USUBJID VISIT VSTESTCD $1024;
STUDYID=StudyOID; USUBJID=SubjectKey; VISIT=catx(' ','VISIT',scan(StudyEventOID,-1,'T')); if StudyEventOID='SE.VISIT1' then VISIT='VISIT 1'; if StudyEventOID='SE.VISIT2' then VISIT='VISIT 2'; VSTESTCD=scan(ItemOID,-1,'.'); VSREPNUM=input(ItemGroupRepeatKey,best32.); VSSTRESN=input(Value,best32.); MEANORD=0; run;
proc sql; create table means as select STUDYID,USUBJID,VISIT,VSTESTCD,mean(VSSTRESN) as VSSTRESN from readings group by STUDYID,USUBJID,VISIT,VSTESTCD; quit;
data meanout; set means; VSREPNUM=.; MEANORD=1; run;
data prepared; set readings meanout; length DOMAIN $2 VSTEST VSORRES VSORRESU VSSTRESC VSDRVFL $1024; DOMAIN='VS'; VSTEST='Diastolic Blood Pressure'; if VSTESTCD='SYSBP' then VSTEST='Systolic Blood Pressure'; VSORRES=strip(put(VSSTRESN,best32.)); VSSTRESC=VSORRES; VSORRESU='mmHg'; VSDRVFL=''; if MEANORD=1 then VSDRVFL='Y'; run;
proc sort data=prepared; by STUDYID USUBJID VISIT VSTESTCD MEANORD VSREPNUM; run;
data result; set prepared; by STUDYID USUBJID; retain VSSEQ;
  if first.USUBJID then VSSEQ=0;
  VSSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM, VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
