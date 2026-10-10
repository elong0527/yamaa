/* opensas reference solution for the yamaa benchmark sdtm-lb-multiform.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data odm;
  length StudyOID MetaDataVersionOID SubjectKey StudyEventOID StudyEventRepeatKey ItemGroupOID ItemGroupRepeatKey ItemOID Value $1024;
  infile "/app/input/odm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input StudyOID : $char1024. MetaDataVersionOID : $char1024. SubjectKey : $char1024. StudyEventOID : $char1024. StudyEventRepeatKey : $char1024. ItemGroupOID : $char1024. ItemGroupRepeatKey : $char1024. ItemOID : $char1024. Value : $char1024.;
run;

data dateitems; set odm; if scan(ItemOID,-1,'.') in ('LBDTC','BXDTC','SALDTC','TSDTC'); run;
proc sql; create table dates as select StudyOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,ItemGroupOID,ItemGroupRepeatKey,max(Value) as LBDTC from dateitems group by StudyOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,ItemGroupOID,ItemGroupRepeatKey;
create table joined as select a.*,b.LBDTC from odm a inner join dates b on a.StudyOID=b.StudyOID and a.SubjectKey=b.SubjectKey and a.StudyEventOID=b.StudyEventOID and a.StudyEventRepeatKey=b.StudyEventRepeatKey and a.ItemGroupOID=b.ItemGroupOID and a.ItemGroupRepeatKey=b.ItemGroupRepeatKey; quit;
data prepared; set joined; length DOMAIN $2 STUDYID USUBJID VISIT LBTESTCD LBTEST LBCAT LBSPEC LBLOC LBORRES LBORRESU LBSTRESC LBSTRESU LBSTAT KEY EVENT $1024;
KEY=scan(ItemOID,-1,'.'); EVENT=scan(StudyEventOID,-1,'.'); if not missing(Value) and KEY in ('VITD','IL13_LES','IL13_NONLES','SAL_CAMP','TS_CAMP_LES','TS_CAMP_NONLES');
DOMAIN='LB'; STUDYID=StudyOID; USUBJID=SubjectKey; LBORRES=Value; LBSTRESC=Value; LBSTRESN=.; LBORRESU='ng/mL'; LBLOC=''; LBSTAT=''; LOCORD=2;
if Value ne 'NOT DONE' then LBSTRESN=input(Value,?? best32.);
if KEY='VITD' then do; LBTESTCD='VITD25OH'; LBTEST='25-Hydroxyvitamin D'; LBCAT='CHEMISTRY'; LBSPEC='SERUM'; end;
else if KEY in ('IL13_LES','IL13_NONLES') then do; LBTESTCD='IL13'; LBTEST='Interleukin 13 mRNA'; LBCAT='GENE EXPRESSION'; LBSPEC='SKIN BIOPSY'; LBORRESU='CYCLE'; end;
else do; LBTESTCD='CAMPPRO'; LBTEST='Cathelicidin Protein'; LBCAT='ANTIMICROBIAL PEPTIDE'; if KEY='SAL_CAMP' then LBSPEC='SALIVA'; else LBSPEC='TAPE STRIP'; end;
if KEY in ('IL13_LES','TS_CAMP_LES') then do; LBLOC='LESIONAL'; LOCORD=0; end;
if KEY in ('IL13_NONLES','TS_CAMP_NONLES') then do; LBLOC='NON-LESIONAL'; LOCORD=1; end;
VISITNUM=.; if EVENT='SCRN' then do; VISIT='SCREENING'; VISITNUM=1; end; if EVENT='BL' then do; VISIT='BASELINE'; VISITNUM=2; end; if EVENT='D21' then do; VISIT='DAY 21'; VISITNUM=3; end; if EVENT='UNSCH' then do; VISIT='UNSCHEDULED'; VISITNUM=2+input(ItemGroupRepeatKey,best32.)/100; end;
LBSTRESU=LBORRESU; if Value='NOT DONE' then do; LBORRES=''; LBORRESU=''; LBSTRESC=''; LBSTRESN=.; LBSTRESU=''; LBSTAT='NOT DONE'; end; run;
proc sort data=prepared; by STUDYID USUBJID LBDTC LBTESTCD LBSPEC LOCORD; run;
data result; set prepared; by STUDYID USUBJID; retain LBSEQ;
  if first.USUBJID then LBSEQ=0;
  LBSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, LBSEQ, VISIT, VISITNUM, LBTESTCD, LBTEST, LBCAT, LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBSTAT, LBDTC from result;
quit;
proc export data=final outfile="/app/output/lb.csv" dbms=csv replace; run;
