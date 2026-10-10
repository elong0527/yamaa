/* opensas reference solution for the yamaa benchmark sdtm-pr-procedures.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey ItemGroupOID ItemGroupRepeatKey; run;
data wide;
  length PRTRT PRLOC PRLAT PRDOSU PRSTDTC PRENDTC RTPRSP DOSETEXT $1024;
  retain PRTRT PRLOC PRLAT PRDOSU PRSTDTC PRENDTC RTPRSP DOSETEXT;
  set ordered;

  by StudyOID SubjectKey ItemGroupOID ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    PRTRT=''; PRLOC=''; PRLAT=''; PRDOSU=''; PRSTDTC=''; PRENDTC=''; RTPRSP=''; DOSETEXT='';
  end;
    if ItemOID='IT.PR.PRTRT' then PRTRT=Value;
  if ItemOID='IT.PR.PRLOC' then PRLOC=Value;
  if ItemOID='IT.PR.PRLAT' then PRLAT=Value;
  if ItemOID='IT.PR.PRDOSU' then PRDOSU=Value;
  if ItemOID='IT.PR.PRSTDTC' then PRSTDTC=Value;
  if ItemOID='IT.PR.PRENDTC' then PRENDTC=Value;
  if ItemOID='IT.PR.RTPRSP' then RTPRSP=Value;
  if ItemOID='IT.PR.PRDOSE' then DOSETEXT=Value;
  if last.ItemGroupRepeatKey then output;
run;
data prepared; set wide; length DOMAIN $2 STUDYID USUBJID PRCAT PRPRESP PROCCUR $1024;
DOMAIN='PR'; STUDYID=StudyOID; USUBJID=catx('-',StudyOID,SubjectKey); PRPRESP=''; PROCCUR=''; PRDOSE=.;
if ItemGroupOID='IG.PR.SURGERY' then do; PRCAT='PRIOR CANCER SURGERY'; PRENDTC=''; end;
else if ItemGroupOID='IG.PR.RADIOTHERAPY' then do; PRCAT='PRIOR RADIOTHERAPY'; PRPRESP='Y'; PROCCUR=RTPRSP; PRLOC=''; PRLAT=''; if RTPRSP='Y' then PRDOSE=input(DOSETEXT,best32.); else do; PRDOSU=''; PRSTDTC=''; PRENDTC=''; end; end; else delete; run;
proc sort data=prepared; by STUDYID USUBJID PRSTDTC PRTRT; run;
data result; set prepared; by STUDYID USUBJID; retain PRSEQ;
  if first.USUBJID then PRSEQ=0;
  PRSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, PRSEQ, PRTRT, PRCAT, PRPRESP, PROCCUR, PRLOC, PRLAT, PRDOSE, PRDOSU, PRSTDTC, PRENDTC from result;
quit;
proc export data=final outfile="/app/output/pr.csv" dbms=csv replace; run;
