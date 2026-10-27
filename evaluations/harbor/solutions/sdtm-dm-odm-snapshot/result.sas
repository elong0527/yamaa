/* opensas reference solution for the yamaa benchmark sdtm-dm-odm-snapshot.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

/* opensas v0.7.1 reads no XML. The runner must stage odm.xml as odm.csv,
   one row per collected item in file order, without deriving any values:
   the item's study OID, subject key, item OID and value, and IsNull as
   true or false. Audit records, signatures and comments give no rows. */
data items;
  length StudyOID SubjectKey ItemOID Value IsNull $1024;
  infile '/app/input/odm.csv' dsd dlm=',' firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. SubjectKey : $char1024. ItemOID : $char1024.
        Value : $char1024. IsNull : $char1024.;
run;

data dm;
  length DOMAIN $2 STUDYID USUBJID SUBJID BRTHDTC AGEU SEX $1024;
  retain BRTHDTC AGE SEX;
  set items;
  by StudyOID SubjectKey notsorted;
  if first.SubjectKey then do;
    BRTHDTC = '';
    AGE = .;
    SEX = '';
  end;
  if IsNull ne 'true' then do;
    if ItemOID = 'IT.DM.BRTHDAT' then BRTHDTC = Value;
    if ItemOID = 'IT.DM.AGE' then AGE = input(Value, best32.);
    if ItemOID = 'IT.DM.SEX' then SEX = Value;
  end;
  if last.SubjectKey then do;
    DOMAIN = 'DM';
    STUDYID = StudyOID;
    USUBJID = catx('-', StudyOID, SubjectKey);
    SUBJID = SubjectKey;
    if missing(AGE) then AGEU = '';
    else AGEU = 'YEARS';
    output;
  end;
  keep DOMAIN STUDYID USUBJID SUBJID BRTHDTC AGE AGEU SEX;
run;

proc sql;
  create table final as
    select DOMAIN, STUDYID, USUBJID, SUBJID, BRTHDTC, AGE, AGEU, SEX from dm;
quit;
proc export data=final outfile='/app/output/dm.csv' dbms=csv replace; run;
