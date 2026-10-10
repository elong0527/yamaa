/* opensas reference solution for the yamaa benchmark sdtm-vs-collected-form.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. FormOID : $char1024. FormRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

data vs_tests;
  length VSTESTCD VSTEST TESTORD HAS_POSITION HAS_METHOD HAS_NOTDONE $1024;
  infile "/app/input/vs_tests.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input VSTESTCD : $char1024. VSTEST : $char1024. TESTORD : $char1024. HAS_POSITION : $char1024. HAS_METHOD : $char1024. HAS_NOTDONE : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey; run;
data visits;
  length VSDTC POSITION TEMPMETHOD BPNOTDONE BPREASND $1024;
  retain VSDTC POSITION TEMPMETHOD BPNOTDONE BPREASND;
  set ordered;

  by StudyOID SubjectKey StudyEventOID StudyEventRepeatKey FormOID FormRepeatKey;
  if first.FormRepeatKey then do;
    VSDTC=''; POSITION=''; TEMPMETHOD=''; BPNOTDONE=''; BPREASND='';
  end;
    if ItemOID='IT.VS.VSDTC' then VSDTC=Value;
  if ItemOID='IT.VS.POSITION' then POSITION=Value;
  if ItemOID='IT.VS.TEMPMETHOD' then TEMPMETHOD=Value;
  if ItemOID='IT.VS.BPNOTDONE' then BPNOTDONE=Value;
  if ItemOID='IT.VS.BPREASND' then BPREASND=Value;
  if last.FormRepeatKey then output;
run;
data tests; set vs_tests; length RESULTOID UNITOID $1024; RESULTOID=cats('IT.VS.',VSTESTCD); UNITOID=cats(RESULTOID,'U'); ORD=input(TESTORD,best32.); run;
proc sql; create table grid as select a.*,b.* from visits a cross join tests b;
create table joined as select a.*,b.Value as RESULTTEXT,c.Value as UNITTEXT from grid a left join odm b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey and a.FormOID=b.FormOID and a.FormRepeatKey=b.FormRepeatKey and a.RESULTOID=b.ItemOID left join odm c on a.StudyOID=c.StudyOID and a.SubjectKey=c.SubjectKey and a.StudyEventOID=c.StudyEventOID and a.StudyEventRepeatKey=c.StudyEventRepeatKey and a.FormOID=c.FormOID and a.FormRepeatKey=c.FormRepeatKey and a.UNITOID=c.ItemOID; quit;
data prepared; set joined; length DOMAIN $2 STUDYID USUBJID VISIT VSORRES VSORRESU VSSTRESC VSSTRESU VSPOS VSMETHOD VSSTAT VSREASND $1024;
DOMAIN='VS'; STUDYID=StudyOID; USUBJID=SubjectKey; VISIT=StudyEventOID; VSORRES=''; VSORRESU=''; VSSTRESN=.; VSSTRESC=''; VSSTRESU=''; VSPOS=''; VSMETHOD=''; VSSTAT=''; VSREASND='';
if HAS_NOTDONE='1' and BPNOTDONE='Y' then do; VSSTAT='NOT DONE'; VSREASND=BPREASND; end;
else if not missing(RESULTTEXT) then do; VSORRES=RESULTTEXT; VSORRESU=UNITTEXT; VSSTRESN=input(RESULTTEXT,best32.); VSSTRESC=strip(put(VSSTRESN,best15.)); VSSTRESU=UNITTEXT; if HAS_POSITION='1' then VSPOS=POSITION; if HAS_METHOD='1' then VSMETHOD=TEMPMETHOD; end; run;
proc sort data=prepared; by STUDYID USUBJID VSDTC ORD; run;
data result; set prepared; by STUDYID USUBJID; retain VSSEQ;
  if first.USUBJID then VSSEQ=0;
  VSSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSSTRESU, VSPOS, VSMETHOD, VSSTAT, VSREASND, VSDTC from result;
quit;
proc export data=final outfile="/app/output/vs.csv" dbms=csv replace; run;
