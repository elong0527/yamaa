/* SAS language reference solution for the yamaa benchmark adam-adrs-response-records.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024.;
run;

data rs;
  length STUDYID USUBJID RSTESTCD RSEVAL RSDTC RSSTRESC $1024;
  infile "/app/input/rs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RSSEQ RSTESTCD : $char1024. RSEVAL : $char1024. RSDTC : $char1024. RSSTRESC : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDT from rs as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID where a.RSTESTCD='OVRLRESP' and a.RSEVAL='INVESTIGATOR'; quit;
data assessments;
  length PARAMCD $8 PARAM $40 ADT $10 AVALC $16;
  set joined; PARAMCD='OVR'; PARAM='Overall Response by Investigator';
  ADT=RSDTC;
  if lengthn(RSDTC)=7 then ADT=cats(RSDTC,'-01');
  if lengthn(RSDTC)=4 then ADT=cats(RSDTC,'-01-01');
  if not missing(ADT) and not missing(TRTSDT) then do;
    ADY=input(ADT,yymmdd10.)-input(TRTSDT,yymmdd10.); if ADY>=0 then ADY=ADY+1;
  end;
  AVALC=RSSTRESC; select(AVALC); when('CR') AVAL=1; when('PR') AVAL=2; when('SD') AVAL=3; when('NON-CR/NON-PD') AVAL=4; when('PD') AVAL=5; when('NE') AVAL=6; otherwise AVAL=.; end;
run;
proc sort data=assessments; by STUDYID USUBJID ADT descending AVAL RSSEQ; run;
data result; length ANL01FL $1; set assessments; by STUDYID USUBJID ADT; if first.ADT then ANL01FL='Y'; run;

proc sql;
  create table final as select STUDYID, USUBJID, RSSEQ, PARAMCD, PARAM, RSDTC, ADT, ADY, AVALC, AVAL, ANL01FL from result;
quit;
proc export data=final outfile="/app/output/adrs.csv" dbms=csv replace; run;
