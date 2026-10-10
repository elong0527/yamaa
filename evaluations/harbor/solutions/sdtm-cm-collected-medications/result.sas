/* SAS language reference solution for the yamaa benchmark sdtm-cm-collected-medications.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey ItemGroupRepeatKey; run;
data wide;
  length CMTRT CMINDC CMDOSU CMDOSFRQ CMROUTE CMSTDTC CMENDTC CMPRIOR CMONGO DOSETEXT $1024;
  retain CMTRT CMINDC CMDOSU CMDOSFRQ CMROUTE CMSTDTC CMENDTC CMPRIOR CMONGO DOSETEXT;
  set ordered;
  where ItemGroupOID='IG.CM';
  by StudyOID SubjectKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    CMTRT=''; CMINDC=''; CMDOSU=''; CMDOSFRQ=''; CMROUTE=''; CMSTDTC=''; CMENDTC=''; CMPRIOR=''; CMONGO=''; DOSETEXT='';
  end;
    if ItemOID='IT.CM.CMTRT' then CMTRT=Value;
  if ItemOID='IT.CM.CMINDC' then CMINDC=Value;
  if ItemOID='IT.CM.CMDOSU' then CMDOSU=Value;
  if ItemOID='IT.CM.CMDOSFRQ' then CMDOSFRQ=Value;
  if ItemOID='IT.CM.CMROUTE' then CMROUTE=Value;
  if ItemOID='IT.CM.CMSTDTC' then CMSTDTC=Value;
  if ItemOID='IT.CM.CMENDTC' then CMENDTC=Value;
  if ItemOID='IT.CM.CMPRIOR' then CMPRIOR=Value;
  if ItemOID='IT.CM.CMONGO' then CMONGO=Value;
  if ItemOID='IT.CM.CMDSTXT' then DOSETEXT=Value;
  if last.ItemGroupRepeatKey then output;
run;
data result; set wide; length DOMAIN $2 STUDYID USUBJID CMDOSTXT CMSTRTPT CMSTTPT CMENRTPT CMENTPT $1024;
DOMAIN='CM'; STUDYID=StudyOID; USUBJID=SubjectKey; CMSEQ=input(ItemGroupRepeatKey,best32.);
if prxmatch('/^[+-]?[0-9]+(\.[0-9]+)?$/',strip(DOSETEXT)) then CMDOSE=input(DOSETEXT,best32.); else CMDOSTXT=DOSETEXT;
if CMDOSFRQ='Once daily' then CMDOSFRQ='QD'; if CMDOSFRQ='Twice daily' then CMDOSFRQ='BID'; if CMROUTE='By mouth' then CMROUTE='ORAL';
if CMPRIOR='Y' then do; CMSTRTPT='BEFORE'; CMSTTPT='SCREENING'; end;
if CMONGO='Y' then do; CMENDTC=''; CMENRTPT='ONGOING'; CMENTPT='END OF STUDY'; end; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMINDC, CMDOSE, CMDOSTXT, CMDOSU, CMDOSFRQ, CMROUTE, CMSTDTC, CMENDTC, CMSTRTPT, CMSTTPT, CMENRTPT, CMENTPT from result;
quit;
proc export data=final outfile="/app/output/cm.csv" dbms=csv replace; run;
