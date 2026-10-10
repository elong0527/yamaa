/* opensas reference solution for the yamaa benchmark sdtm-cm-odm-repeated.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey; run;
data wide;
  length CMTRT CMSTDTC CMENDTC CMROUTE CMINDC CMONGO $1024;
  retain CMTRT CMSTDTC CMENDTC CMROUTE CMINDC CMONGO;
  set ordered;
  where ItemGroupOID='IG.CM';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    CMTRT=''; CMSTDTC=''; CMENDTC=''; CMROUTE=''; CMINDC=''; CMONGO='';
  end;
    if ItemOID='IT.CM.CMTRT' then CMTRT=Value;
  if ItemOID='IT.CM.CMSTDTC' then CMSTDTC=Value;
  if ItemOID='IT.CM.CMENDTC' then CMENDTC=Value;
  if ItemOID='IT.CM.CMROUTE' then CMROUTE=Value;
  if ItemOID='IT.CM.CMINDC' then CMINDC=Value;
  if ItemOID='IT.CM.CMONGO' then CMONGO=Value;
  if last.ItemGroupRepeatKey then output;
run;
data prepared; set wide; if not missing(CMTRT); length DOMAIN $2 STUDYID USUBJID $1024; DOMAIN='CM'; STUDYID=StudyOID; USUBJID=SubjectKey;
if CMONGO='Y' then CMENDTC=''; VISITORD=2; if StudyEventOID='SCREENING' then VISITORD=0; if StudyEventOID='BASELINE' then VISITORD=1;
VISITREP=input(StudyEventRepeatKey,best32.); IGREP=input(ItemGroupRepeatKey,best32.); run;
proc sort data=prepared; by STUDYID USUBJID VISITORD VISITREP IGREP; run;
data result; set prepared; by STUDYID USUBJID; retain CMSEQ;
  if first.USUBJID then CMSEQ=0;
  CMSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDTC, CMENDTC, CMROUTE, CMINDC from result;
quit;
proc export data=final outfile="/app/output/cm.csv" dbms=csv replace; run;
