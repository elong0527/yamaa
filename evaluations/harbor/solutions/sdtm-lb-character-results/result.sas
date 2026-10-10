/* opensas reference solution for the yamaa benchmark sdtm-lb-character-results.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. FormOID : $char1024. FormRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey FormOID FormRepeatKey; run;
data wide;
  length LBORRES LBORRESU LBDTC $1024;
  retain LBORRES LBORRESU LBDTC;
  set ordered;

  by StudyOID SubjectKey FormOID FormRepeatKey;
  if first.FormRepeatKey then do;
    LBORRES=''; LBORRESU=''; LBDTC='';
  end;
    if ItemOID='IT.LB.RESULT' then LBORRES=Value;
  if ItemOID='IT.LB.LBORRESU' then LBORRESU=Value;
  if ItemOID='IT.LB.LBDTC' then LBDTC=Value;
  if last.FormRepeatKey then output;
run;
data prepared; set wide; if not missing(LBORRES); length DOMAIN $2 STUDYID USUBJID LBTESTCD LBTEST LBSTRESC LBSTRESU LBNRIND TOKEN $1024;
DOMAIN='LB'; STUDYID=StudyOID; USUBJID=SubjectKey; LBSTRESU=LBORRESU; LBSTRESC=LBORRES; LBNRIND=''; LBSTRESN=.; TOKEN=lowcase(compress(LBORRES,' '));
if TOKEN in ('negative','neg') then LBSTRESC='NEGATIVE'; if TOKEN in ('trace','tr') then LBSTRESC='TRACE'; if TOKEN in ('1+','+1','1plus') then LBSTRESC='1+'; if TOKEN in ('2+','+2','2plus') then LBSTRESC='2+';
if prxmatch('/^[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?$/',strip(LBORRES)) and not (LBSTRESC in ('NEGATIVE','TRACE','1+','2+')) then LBSTRESN=input(LBORRES,best32.);
if substr(strip(LBORRES),1,1)='<' then LBNRIND='LOW'; if substr(strip(LBORRES),1,1)='>' then LBNRIND='HIGH';
if FormOID='FO.LB_PROT' then do; LBTESTCD='PROT'; LBTEST='Protein'; TESTORD=1; end;
else if FormOID='FO.LB_CK' then do; LBTESTCD='CK'; LBTEST='Creatine Kinase'; TESTORD=2; end;
else if FormOID='FO.LB_GLUC' then do; LBTESTCD='GLUC'; LBTEST='Glucose'; TESTORD=3; end;
else if FormOID='FO.LB_KETON' then do; LBTESTCD='KETON'; LBTEST='Ketones'; TESTORD=4; end;
else if FormOID='FO.LB_CREAT' then do; LBTESTCD='CREAT'; LBTEST='Creatinine'; TESTORD=5; end; else delete;
REP=input(FormRepeatKey,best32.); run;
proc sort data=prepared; by STUDYID USUBJID TESTORD REP; run;
data result; set prepared; by STUDYID USUBJID; retain LBSEQ;
  if first.USUBJID then LBSEQ=0;
  LBSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBNRIND, LBDTC from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
