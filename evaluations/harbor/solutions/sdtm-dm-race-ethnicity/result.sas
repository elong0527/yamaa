/* SAS language reference solution for the yamaa benchmark sdtm-dm-race-ethnicity.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

proc sql;
create table subjects as select distinct StudyOID as STUDYID,SubjectKey as USUBJID from odm;
create table races as select distinct StudyOID as STUDYID,SubjectKey as USUBJID,Value from odm where ItemOID='IT.DM.RACE' and not missing(Value);
quit;
data mapped; set races; length QVAL $1024; if Value in ('White','Asian','Black or African American','American Indian or Alaska Native','Native Hawaiian or Other Pacific Islander') then QVAL=upcase(Value);
else if Value='Other, specify: Fijian' then QVAL='OTHER'; else if Value='Subject refused' then QVAL='UNKNOWN'; else if Value='Not reported' then QVAL='NOT REPORTED'; run;
proc sql; create table racecounts as select STUDYID,USUBJID,count(*) as NRACE from mapped group by STUDYID,USUBJID; quit;
proc sort data=odm out=ordered; by StudyOID SubjectKey; run;
data ethnicity;
  length ETHNIC $1024;
  retain ETHNIC;
  set ordered;

  by StudyOID SubjectKey;
  if first.SubjectKey then do;
    ETHNIC='';
  end;
    if ItemOID='IT.DM.ETHNIC' then ETHNIC=Value;
  if last.SubjectKey then output;
run;
data ethnic; set ethnicity; length STUDYID USUBJID $1024; STUDYID=StudyOID; USUBJID=SubjectKey;
if ETHNIC in ('Hispanic or Latino','Not Hispanic or Latino','Not reported','Unknown') then ETHNIC=upcase(ETHNIC); else ETHNIC=''; run;
proc sql;
create table dmjoined as select a.*,b.NRACE,c.QVAL,d.ETHNIC from subjects a left join racecounts b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join mapped c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID and b.NRACE=1 left join ethnic d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID;
create table multirace as select a.* from mapped a inner join racecounts b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where b.NRACE>1;
quit;
data dmout; set dmjoined; length DOMAIN $2 SUBJID RACE $1024; DOMAIN='DM'; SUBJID=USUBJID; RACE=QVAL; if NRACE>1 then RACE='MULTIPLE'; keep DOMAIN STUDYID USUBJID SUBJID RACE ETHNIC; run;
proc sort data=multirace; by STUDYID USUBJID Value; run;
data suppout; set multirace; by STUDYID USUBJID; length RDOMAIN $2 IDVAR IDVARVAL QNAM QLABEL QORIG QEVAL $1024; retain N;
if first.USUBJID then N=0; N+1; RDOMAIN='DM'; IDVAR='USUBJID'; IDVARVAL=USUBJID; QNAM=cats('RACE',put(N,best32.)); QLABEL=catx(' ','Race',strip(put(N,best32.))); QORIG='CRF'; QEVAL=''; run;
proc sql; create table suppdmout as select STUDYID,RDOMAIN,USUBJID,IDVAR,IDVARVAL,QNAM,QLABEL,QVAL,QORIG,QEVAL from suppout; quit;
proc export data=dmout outfile='/app/output/dm.csv' dbms=csv replace; run;
proc export data=suppdmout outfile='/app/output/suppdm.csv' dbms=csv replace; run;
