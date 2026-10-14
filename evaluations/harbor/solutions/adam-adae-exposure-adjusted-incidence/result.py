import csv
from collections import defaultdict
from datetime import date, timedelta

subjects = list(csv.DictReader(open('/app/input/adsl.csv')))
events = list(csv.DictReader(open('/app/input/adae.csv')))
terms = ['DIARRHOEA', 'HEADACHE']
risk, totals = [], defaultdict(lambda: [0, 0])
for subject in subjects:
    start = date.fromisoformat(subject['TRTSDT'])
    end = date.fromisoformat(subject['TRTEDT']) + timedelta(days=30)
    for term in terms:
        onsets = [date.fromisoformat(e['ASTDT']) for e in events
                  if e['STUDYID'] == subject['STUDYID']
                  and e['USUBJID'] == subject['USUBJID']
                  and e['AEDECOD'] == term and e['ASTDT']]
        eligible = [onset for onset in onsets if start <= onset <= end]
        first = min(eligible) if eligible else None
        days = ((first or end) - start).days + 1
        occurred = int(first is not None)
        risk.append([subject['STUDYID'], subject['USUBJID'],
                     subject['TRT01A'], term,
                     first.isoformat() if first else '', occurred, days])
        key = subject['STUDYID'], subject['TRT01A'], term
        totals[key][0] += occurred
        totals[key][1] += days
rates = []
for key, (count, days) in sorted(totals.items()):
    years = days / 365.25
    rates.append([*key, count, days, years, 100 * count / years])
outputs = [
    ('adrisk.csv', ['STUDYID', 'USUBJID', 'TRT01A', 'AEDECOD', 'FIRSTDT',
                    'EVENTN', 'RISKDAYS'], sorted(risk)),
    ('adinc.csv', ['STUDYID', 'TRT01A', 'AEDECOD', 'NSUBJ', 'PERSONDAYS',
                   'PERSONYRS', 'EAIR100'], rates),
]
for filename, columns, rows in outputs:
    with open('/app/output/' + filename, 'w', newline='') as output:
        writer = csv.writer(output)
        writer.writerow(columns)
        writer.writerows(rows)
