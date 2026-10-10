/* opensas reference solution for the yamaa benchmark sdtm-cm-whodrug-coding.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data coding_output;
  length STUDYID USUBJID DRUG_RECORD_NO DRUG_CODE ATC_CODE $1024;
  infile "/app/input/coding_output.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ DRUG_RECORD_NO : $char1024. DRUG_CODE : $char1024. ATC_CODE : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

data whodrug_extract;
  length DRUG_RECORD_NO PREFERRED_NAME DRUG_CODE ATC_CODE ATC_CLASS_CODE ATC_CLASS_NAME $1024;
  infile "/app/input/whodrug_extract.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input DRUG_RECORD_NO : $char1024. PREFERRED_NAME : $char1024. DRUG_CODE : $char1024. ATC_CODE : $char1024. ATC_CLASS_CODE : $char1024. ATC_CLASS_NAME : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey ItemGroupRepeatKey; run;
data wide;
  length CMTRT $1024;
  retain CMTRT;
  set ordered;
  where ItemGroupOID='IG.CM';
  by StudyOID SubjectKey ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    CMTRT='';
  end;
    if ItemOID='IT.CM.CMTRT' then CMTRT=Value;
  if last.ItemGroupRepeatKey then output;
run;
data medications; set wide; length DOMAIN $2 STUDYID USUBJID $1024; DOMAIN='CM'; STUDYID=StudyOID; USUBJID=SubjectKey; CMSEQ=input(ItemGroupRepeatKey,best32.); run;
proc sql; create table result as select a.*,c.PREFERRED_NAME as CMDECOD,c.ATC_CLASS_NAME as CMCLAS,c.ATC_CLASS_CODE as CMCLASCD from medications a
left join coding_output b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.CMSEQ=b.CMSEQ
left join whodrug_extract c on b.DRUG_RECORD_NO=c.DRUG_RECORD_NO and b.DRUG_CODE=c.DRUG_CODE and b.ATC_CODE=c.ATC_CODE; quit;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, CMCLAS, CMCLASCD from result;
quit;
proc export data=final outfile="/app/output/cm.csv" dbms=csv replace; run;
