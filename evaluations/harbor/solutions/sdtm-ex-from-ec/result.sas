/* SAS language reference solution for the yamaa benchmark sdtm-ex-from-ec.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data kit_list;
  length KIT TREATMENT DOSE_MG $1024;
  infile "/app/input/kit_list.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input KIT : $char1024. TREATMENT : $char1024. DOSE_MG : $char1024.;
run;

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. FormOID : $char1024. FormRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormRepeatKey; run;
data doses;
  length ECTRT ECSTDTC ECENDTC ECOCCUR FORM TABLETS STRENGTHMG DOSEMKG KIT AUCTARGET $1024;
  retain ECTRT ECSTDTC ECENDTC ECOCCUR FORM TABLETS STRENGTHMG DOSEMKG KIT AUCTARGET;
  set ordered;
  where FormOID='FO.EC';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormRepeatKey;
  if first.FormRepeatKey then do;
    ECTRT=''; ECSTDTC=''; ECENDTC=''; ECOCCUR=''; FORM=''; TABLETS=''; STRENGTHMG=''; DOSEMKG=''; KIT=''; AUCTARGET='';
  end;
    if ItemOID='IT.EC.ECTRT' then ECTRT=Value;
  if ItemOID='IT.EC.ECSTDTC' then ECSTDTC=Value;
  if ItemOID='IT.EC.ECENDTC' then ECENDTC=Value;
  if ItemOID='IT.EC.ECOCCUR' then ECOCCUR=Value;
  if ItemOID='IT.EC.FORM' then FORM=Value;
  if ItemOID='IT.EC.TABLETS' then TABLETS=Value;
  if ItemOID='IT.EC.STRENGTHMG' then STRENGTHMG=Value;
  if ItemOID='IT.EC.DOSEMKG' then DOSEMKG=Value;
  if ItemOID='IT.EC.KIT' then KIT=Value;
  if ItemOID='IT.EC.AUCTARGET' then AUCTARGET=Value;
  if last.FormRepeatKey then output;
run;
proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey; run;
data weights;
  length VISITWEIGHT $1024;
  retain VISITWEIGHT;
  set ordered;
  where FormOID='FO.VS';
  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey;
  if first.StudyEventRepeatKey then do;
    VISITWEIGHT='';
  end;
    if ItemOID='IT.VS.WEIGHT' then VISITWEIGHT=Value;
  if last.StudyEventRepeatKey then output;
run;
proc sql;
create table joined as select a.*,b.VISITWEIGHT,c.TREATMENT,c.DOSE_MG from doses a left join weights b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey left join kit_list c on a.KIT=c.KIT; quit;
data prepared; set joined; if ECOCCUR='Y'; length DOMAIN $2 STUDYID USUBJID EXTRT EXDOSU EXSTDTC EXENDTC $1024;
DOMAIN='EX'; STUDYID=StudyOID; USUBJID=SubjectKey; EXTRT=ECTRT; EXSTDTC=ECSTDTC; EXENDTC=ECENDTC; EXDOSU='mg'; EXDOSE=.;
if FORM='TABLET' then EXDOSE=input(TABLETS,best32.)*input(STRENGTHMG,best32.);
if FORM='INFUSION' then EXDOSE=input(DOSEMKG,best32.)*input(VISITWEIGHT,best32.);
if FORM='KITDOSE' then do; EXTRT=TREATMENT; EXDOSE=input(DOSE_MG,best32.); end;
if FORM='AUCDOSE' then do; EXDOSE=input(AUCTARGET,best32.); EXDOSU='AUC'; end;
FORMREP=input(FormRepeatKey,best32.); run;
proc sort data=prepared; by STUDYID USUBJID EXSTDTC FORMREP; run;
data result; set prepared; by STUDYID USUBJID; retain EXSEQ;
  if first.USUBJID then EXSEQ=0;
  EXSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC from result;
quit;
proc export data=final outfile="/app/output/ex.csv" dbms=csv replace; run;
