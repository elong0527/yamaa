/* opensas reference solution for the yamaa benchmark sdtm-ae-meddra-hierarchy.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae_raw;
  length STUDYID USUBJID AETERM AELLTCD $1024;
  infile "/app/input/ae_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AELLTCD : $char1024.;
run;

data meddra_synthetic;
  length LLTCD LLTNAME PTCD PTNAME HLTCD HLTNAME HLGTCD HLGTNAME SOCCD SOCNAME PRIMARY_SOC $1024;
  infile "/app/input/meddra_synthetic.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input LLTCD : $char1024. LLTNAME : $char1024. PTCD : $char1024. PTNAME : $char1024. HLTCD : $char1024. HLTNAME : $char1024. HLGTCD : $char1024. HLGTNAME : $char1024. SOCCD : $char1024. SOCNAME : $char1024. PRIMARY_SOC : $char1024.;
run;

proc sql;
  create table result as
  select 'AE' as DOMAIN,a.*,b.LLTNAME as AELLT,b.PTNAME as AEDECOD,b.PTCD as AEPTCD,b.HLTNAME as AEHLT,b.HLTCD as AEHLTCD,b.HLGTNAME as AEHLGT,b.HLGTCD as AEHLGTCD,b.SOCNAME as AEBODSYS,b.SOCCD as AEBODSCD,b.SOCNAME as AESOC,b.SOCCD as AESOCCD from ae_raw a left join meddra_synthetic b on a.AELLTCD=b.LLTCD and b.PRIMARY_SOC='Y';
quit;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AELLT, AELLTCD, AEDECOD, AEPTCD, AEHLT, AEHLTCD, AEHLGT, AEHLGTCD, AEBODSYS, AEBODSCD, AESOC, AESOCCD from result;
quit;
proc export data=final outfile="/app/output/ae.csv" dbms=csv replace; run;
