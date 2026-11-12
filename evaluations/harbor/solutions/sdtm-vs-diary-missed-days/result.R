# Reference solution for the yamaa benchmark sdtm-vs-diary-missed-days (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

suppressPackageStartupMessages({
  library(dplyr)
  library(readr)
})

read_chr <- function(path) {
  read_csv(path, col_types = cols(.default = col_character()),
           na = character(), trim_ws = FALSE)
}

diary <- read_chr("/app/input/diary_days.csv")
temp  <- read_chr("/app/input/temperature.csv")
vac   <- read_chr("/app/input/vaccination.csv")

blank_to_na <- function(x) ifelse(is.na(x) | trimws(x) == "", NA_character_, x)

diary <- diary %>%
  mutate(VSTPTNUM = as.integer(DIARYDAY), VSTPT = DAYLABEL) %>%
  select(STUDYID, VSTPTNUM, VSTPT)

given <- vac %>%
  mutate(VACDAT = blank_to_na(VACDAT)) %>%
  filter(!is.na(VACDAT)) %>%
  mutate(VACDATE = as.Date(VACDAT))

temp <- temp %>%
  mutate(TEMPDAT = blank_to_na(TEMPDAT),
         TEMP = blank_to_na(TEMP),
         TEMPU = blank_to_na(TEMPU)) %>%
  filter(!is.na(TEMPDAT)) %>%
  select(STUDYID, USUBJID, VSDTC = TEMPDAT, VSORRES = TEMP, VSORRESU = TEMPU)

vs <- given %>%
  inner_join(diary, by = "STUDYID", relationship = "many-to-many") %>%
  mutate(VSDTC = format(VACDATE + (VSTPTNUM - 1L), "%Y-%m-%d"),
         VSTPTREF = VISIT,
         VSRFTDTC = VACDAT) %>%
  left_join(temp, by = c("STUDYID", "USUBJID", "VSDTC")) %>%
  mutate(
    DOMAIN = "VS",
    VSTESTCD = "TEMP",
    VSTEST = "Temperature",
    entered = !is.na(VSORRES),
    VSSTAT = ifelse(entered, NA_character_, "NOT DONE"),
    VSREASND = ifelse(entered, NA_character_, "NOT ENTERED")
  ) %>%
  arrange(USUBJID, VACDATE, VSTPTNUM) %>%
  group_by(USUBJID) %>%
  mutate(VSSEQ = row_number()) %>%
  ungroup() %>%
  select(DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSTEST, VSORRES, VSORRESU,
         VSSTAT, VSREASND, VSDTC, VSTPT, VSTPTNUM, VSTPTREF, VSRFTDTC)

dir.create("/app/output", showWarnings = FALSE)
write_csv(vs, "/app/output/vs.csv", na = "")
