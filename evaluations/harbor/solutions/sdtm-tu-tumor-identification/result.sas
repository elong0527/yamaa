/* SAS language reference solution for the yamaa benchmark sdtm-tu-tumor-identification.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data tu_raw;
  length STUDYID USUBJID LESION_CATEGORY LESION_NUM LOCATION LATERALITY METHOD EVALUATOR VISIT TUDTC $1024;
  infile "/app/input/tu_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TUSEQ LESION_CATEGORY : $char1024. LESION_NUM : $char1024. LOCATION : $char1024. LATERALITY : $char1024. METHOD : $char1024. EVALUATOR : $char1024. VISITNUM VISIT : $char1024. TUDTC : $char1024.;
run;

data result; set tu_raw; length DOMAIN $2 TULNKID TUTESTCD TUTEST TUORRES TUSTRESC TULOC TULAT TUMETHOD TUEVAL PREFIX $1024;
DOMAIN='TU'; TUORRES=upcase(LESION_CATEGORY); TUSTRESC=TUORRES; TUTESTCD='TUMIDENT'; TUTEST='Tumor Identification'; TULOC=upcase(LOCATION); TULAT=upcase(LATERALITY); TUMETHOD=METHOD; TUEVAL=EVALUATOR; PREFIX='NEW'; if TUORRES='TARGET' then PREFIX='T'; if TUORRES='NON-TARGET' then PREFIX='NT'; TULNKID=cats(PREFIX,put(input(LESION_NUM,best32.),z2.)); run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, TUSEQ, TULNKID, TUTESTCD, TUTEST, TUORRES, TUSTRESC, TULOC, TULAT, TUMETHOD, TUEVAL, VISITNUM, VISIT, TUDTC from result;
quit;
proc export data=final outfile="/app/output/tu.csv" dbms=csv replace; run;
