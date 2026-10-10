/* opensas reference solution for the yamaa benchmark sdtm-rs-timepoint-response.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID; run;
data visits;
  length AVISIT VISITORDER ADT $1024;
  retain AVISIT VISITORDER ADT;
  set ordered;
  where ItemGroupOID='IG.VISIT';
  by StudyOID SubjectKey StudyEventOID;
  if first.StudyEventOID then do;
    AVISIT=''; VISITORDER=''; ADT='';
  end;
    if ItemOID='IT.VISIT.AVISIT' then AVISIT=Value;
  if ItemOID='IT.VISIT.AVISITN' then VISITORDER=Value;
  if ItemOID='IT.VISIT.ADT' then ADT=Value;
  if last.StudyEventOID then output;
run;
proc sort data=odm out=ordered; by StudyOID SubjectKey StudyEventOID ItemGroupRepeatKey; run;
data targetforms;
  length DIAM $1024;
  retain DIAM;
  set ordered;
  where ItemGroupOID='IG.TRTARGET';
  by StudyOID SubjectKey StudyEventOID ItemGroupRepeatKey;
  if first.ItemGroupRepeatKey then do;
    DIAM='';
  end;
    if ItemOID='IT.TR.LDIAM' then DIAM=Value;
  if last.ItemGroupRepeatKey then output;
run;
data targets; set targetforms; DIAMETER=input(DIAM,?? best32.); run;
proc sql;
create table chosen as select StudyOID,SubjectKey,count(distinct Value) as NCHOSEN from odm where ItemGroupOID='IG.TUTARGET' and StudyEventOID='BASELINE' and ItemOID='IT.TU.TULNKID' and not missing(Value) group by StudyOID,SubjectKey;
create table baseline as select StudyOID,SubjectKey,count(*) as NBASE,count(DIAMETER) as NBASEDONE,sum(DIAMETER) as BASE from targets where StudyEventOID='BASELINE' group by StudyOID,SubjectKey;
create table measures as select StudyOID,SubjectKey,StudyEventOID,count(DIAMETER) as NMEAS,sum(DIAMETER) as TOTAL,max(DIAMETER) as MAXDIAM from targets group by StudyOID,SubjectKey,StudyEventOID;
create table done as select distinct StudyOID,SubjectKey,StudyEventOID from odm where ItemGroupOID in ('IG.TRTARGET','IG.TRNT');
create table joined as select a.*,b.NCHOSEN,c.NBASE,c.NBASEDONE,c.BASE,d.NMEAS,d.TOTAL,d.MAXDIAM,e.StudyEventOID as DONEEVENT from visits a left join chosen b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey left join baseline c on a.StudyOID=c.StudyOID and a.SubjectKey=c.SubjectKey left join measures d on a.StudyOID=d.StudyOID and a.SubjectKey=d.SubjectKey and a.StudyEventOID=d.StudyEventOID left join done e on a.StudyOID=e.StudyOID and a.SubjectKey=e.SubjectKey and a.StudyEventOID=e.StudyEventOID; quit;
data prepared; set joined; length STUDYID USUBJID RSTESTCD RSTEST RSSTRESC RSSTAT $1024; STUDYID=StudyOID; USUBJID=SubjectKey; AVISITN=input(VISITORDER,best32.); RSTESTCD='TRGRESP'; RSTEST='Timepoint Response'; RSSTRESC='NE'; RSSTAT=''; NSELECTED=coalesce(NCHOSEN,NBASE,0);
if missing(DONEEVENT) then do; RSSTRESC=''; RSSTAT='NOT DONE'; end;
else if StudyEventOID ne 'BASELINE' and NSELECTED>0 and NMEAS>=NSELECTED and NBASEDONE=NBASE and not missing(BASE) and BASE ne 0 then do;
if MAXDIAM=0 then RSSTRESC='CR'; else if (BASE-TOTAL)/BASE>=0.30 then RSSTRESC='PR'; else if (TOTAL-BASE)/BASE>=0.20 then RSSTRESC='PD'; else RSSTRESC='SD'; end; run;
proc sort data=prepared; by STUDYID USUBJID AVISITN; run;
data result; set prepared; by STUDYID USUBJID; retain RSSEQ;
  if first.USUBJID then RSSEQ=0;
  RSSEQ+1;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AVISIT, AVISITN, ADT, RSSEQ, RSTESTCD, RSTEST, RSSTRESC, RSSTAT from result;
quit;
proc export data=final outfile="/app/output/rs.csv" dbms=csv replace; run;
