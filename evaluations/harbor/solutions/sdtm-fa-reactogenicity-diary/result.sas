/* SAS language reference solution for the yamaa benchmark sdtm-fa-reactogenicity-diary.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data diary;
  length STUDYID USUBJID DIARYDAY TPT DTC REACTION CATSRC OCCUR SEV DIAMETER DIAMUNIT COMPLETED $1024;
  infile "/app/input/diary.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DIARYDAY : $char1024. TPT : $char1024. DTC : $char1024. REACTION : $char1024. CATSRC : $char1024. OCCUR : $char1024. SEV : $char1024. DIAMETER : $char1024. DIAMUNIT : $char1024. COMPLETED : $char1024.;
run;

data prepared; set diary; if REACTION ne 'TEMPERATURE'; length DOMAIN $2 FATESTCD FATEST FAOBJ FACAT FASCAT FAORRES FAORRESU FASTRESC FASTRESU FASTAT FATPT FADTC $1024;
DOMAIN='FA'; FAOBJ=REACTION; FACAT='REACTOGENICITY'; FASCAT=CATSRC; if CATSRC='LOCAL' then FASCAT='ADMINISTRATION SITE'; FATPT=TPT; FADTC=DTC; DAYORD=input(DIARYDAY,best32.);
FAORRESU=''; FASTRESU=''; FASTRESN=.; FASTAT=''; FAORRES=''; FATESTCD='OCCUR'; FATEST='Occurrence Indicator'; TESTORD=1;
if COMPLETED='Y' then FAORRES=OCCUR; else FASTAT='NOT DONE'; FASTRESC=FAORRES; output;
if COMPLETED='Y' then do;
FATESTCD='SEV'; FATEST='Severity/Intensity'; TESTORD=2; FAORRES='NONE'; if OCCUR='Y' then FAORRES=SEV; FASTRESC=FAORRES; output;
if REACTION in ('REDNESS','SWELLING') and OCCUR='Y' then do; FATESTCD='LDIAM'; FATEST='Longest Diameter'; TESTORD=3; FASTRESN=input(DIAMETER,best32.); FAORRES=strip(put(FASTRESN,best32.)); FASTRESC=FAORRES; FAORRESU=DIAMUNIT; FASTRESU=DIAMUNIT; output; end; end; run;
proc sort data=prepared; by STUDYID USUBJID DAYORD REACTION TESTORD; run;
data result; set prepared; by STUDYID USUBJID; retain FASEQ;
  if first.USUBJID then FASEQ=0;
  FASEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FASCAT, FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FASTAT, FATPT, FADTC from result;
quit;
proc export data=final outfile="/app/output/fa.csv" dbms=csv replace; run;
