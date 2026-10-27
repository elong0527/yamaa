/* SDTM DM: preserve character identifiers and ISO 8601 reference dates. */
data dm;
    length DOMAIN $2 STUDYID $40 USUBJID $80 SUBJID $20
           RFSTDTC $10 SITEID $20 AGE 8 AGEU $5 SEX $1
           RACE $80 COUNTRY $3;
    infile '/app/input/dm_raw.csv' dsd dlm=',' firstobs=2 truncover;
    input STUDYID :$40. USUBJID :$80. SUBJID :$20.
          SITEID :$20. RFSTDTC :$10. AGE
          SEX :$1. RACE :$80. COUNTRY :$3.;
    DOMAIN = 'DM';
    AGEU = 'YEARS';
run;

proc export data=dm outfile='/app/output/dm.csv' dbms=csv replace;
run;
