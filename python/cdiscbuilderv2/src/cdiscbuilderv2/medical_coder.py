"""
Automated Medical Coding Engine for ClinForge.
Standardizes clinical verbatim terms into official MedDRA and WHO Drug Global hierarchies
using multi-tier matching (Exact -> Synonym/Normalized -> Fuzzy Levenshtein -> AI Semantic).
"""

import re
import math
from typing import Any, Dict, List, Optional, Tuple
import polars as pl
from pydantic import BaseModel


# ==================== DATA SCHEMAS ====================

class MedDRARecord(BaseModel):
    verbatim: str
    llt: str
    llt_code: str
    pt: str                 # Preferred Term (AEDECOD)
    pt_code: str            # PT Code (AEPTCD)
    hlt: str                # High Level Term (AEHLT)
    hlt_code: str           # HLT Code (AEHLTCD)
    hlgt: str               # High Level Group Term (AEHLGT)
    hlgt_code: str          # HLGT Code (AEHLGTCD)
    soc: str                # System Organ Class (AEBODSYS / AESOC)
    soc_code: str           # SOC Code (AESOCCD)
    confidence: float = 1.0
    match_tier: str = "EXACT_MATCH"
    needs_review: bool = False


class WHODrugRecord(BaseModel):
    verbatim: str
    preferred_name: str     # Standard Generic / Active Ingredient (CMDECOD)
    drug_record_number: str # Drug Code
    therapeutic_class: str  # Pharmacological / Therapeutic Class (CMCLAS)
    atc_code: str           # Full ATC Code (e.g. N02BE01)
    atc_level1: str         # Anatomical Main Group (e.g. N - Nervous System)
    atc_level2: str         # Therapeutic Subgroup (e.g. N02 - Analgesics)
    atc_level3: str         # Pharmacological Subgroup (e.g. N02B - Other Analgesics and Antipyretics)
    atc_level4: str         # Chemical Subgroup (e.g. N02BE - Anilides)
    confidence: float = 1.0
    match_tier: str = "EXACT_MATCH"
    needs_review: bool = False


# ==================== REFERENCE DICTIONARY KNOWLEDGE BASES ====================

# Standard MedDRA 26.1 Curated Knowledge Base (Adverse Events, Signs, Symptoms, Toxicities, Medical History)
MEDDRA_KNOWLEDGE_BASE: List[Dict[str, Any]] = [
    # General & Administration Site Conditions
    {"pt": "Pyrexia", "pt_code": "10037660", "llt": "Fever", "llt_code": "10016558", "hlt": "Febrile disorders", "hlt_code": "10016560", "hlgt": "Body temperature conditions", "hlgt_code": "10005886", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["fever", "pyrexia", "elevated temperature", "high temp", "febrile", "febrile illness", "hyperthermia"]},
    {"pt": "Fatigue", "pt_code": "10016256", "llt": "Fatigue", "llt_code": "10016256", "hlt": "Asthenic conditions", "hlt_code": "10003549", "hlgt": "General system disorders NEC", "hlgt_code": "10018073", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["fatigue", "tiredness", "exhaustion", "lethargy", "weariness", "malaise", "lack of energy"]},
    {"pt": "Asthenia", "pt_code": "10003549", "llt": "Weakness", "llt_code": "10047896", "hlt": "Asthenic conditions", "hlt_code": "10003549", "hlgt": "General system disorders NEC", "hlgt_code": "10018073", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["asthenia", "generalized weakness", "feeling weak", "loss of strength"]},
    {"pt": "Chills", "pt_code": "10008531", "llt": "Chills", "llt_code": "10008531", "hlt": "Febrile disorders", "hlt_code": "10016560", "hlgt": "Body temperature conditions", "hlgt_code": "10005886", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["chills", "rigors", "shivering", "feeling cold"]},
    {"pt": "Injection site reaction", "pt_code": "10022095", "llt": "Injection site reaction", "llt_code": "10022095", "hlt": "Injection site reactions", "hlt_code": "10022096", "hlgt": "Administration site reactions", "hlgt_code": "10001316", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["injection site erythema", "injection site pain", "injection site swelling", "injection site reaction", "infusion site reaction"]},
    {"pt": "Infusion related reaction", "pt_code": "10051792", "llt": "Infusion related reaction", "llt_code": "10051792", "hlt": "Infusion reactions", "hlt_code": "10068417", "hlgt": "Administration site reactions", "hlgt_code": "10001316", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["infusion related reaction", "irr", "infusion toxicity", "reaction during infusion", "acute infusion reaction"]},
    {"pt": "Pain", "pt_code": "10033371", "llt": "Pain", "llt_code": "10033371", "hlt": "Pain and discomfort NEC", "hlt_code": "10033383", "hlgt": "General system disorders NEC", "hlgt_code": "10018073", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["pain", "generalized pain", "body ache", "discomfort"]},
    {"pt": "Edema peripheral", "pt_code": "10034599", "llt": "Peripheral edema", "llt_code": "10034600", "hlt": "Peripheral oedemas", "hlt_code": "10034604", "hlgt": "General system disorders NEC", "hlgt_code": "10018073", "soc": "General disorders and administration site conditions", "soc_code": "10018065", "synonyms": ["peripheral edema", "edema peripheral", "ankle swelling", "swollen ankles", "leg swelling", "pedal edema", "pitting edema"]},

    # Gastrointestinal Disorders
    {"pt": "Nausea", "pt_code": "10028813", "llt": "Nausea", "llt_code": "10028813", "hlt": "Nausea and vomiting symptoms", "hlt_code": "10028817", "hlgt": "Gastrointestinal signs and symptoms", "hlgt_code": "10017997", "soc": "Gastrointestinal disorders", "soc_code": "10017947", "synonyms": ["nausea", "feeling sick", "queasiness", "upset stomach", "nauseous", "nauseated"]},
    {"pt": "Vomiting", "pt_code": "10047700", "llt": "Vomiting", "llt_code": "10047700", "hlt": "Nausea and vomiting symptoms", "hlt_code": "10028817", "hlgt": "Gastrointestinal signs and symptoms", "hlgt_code": "10017997", "soc": "Gastrointestinal disorders", "soc_code": "10017947", "synonyms": ["vomiting", "emesis", "threw up", "throwing up", "puking", "vomited"]},
    {"pt": "Diarrhoea", "pt_code": "10012735", "llt": "Diarrhea", "llt_code": "10012727", "hlt": "Diarrhoea (excl infective)", "hlt_code": "10012736", "hlgt": "Gastrointestinal motility and defaecation conditions", "hlgt_code": "10017983", "soc": "Gastrointestinal disorders", "soc_code": "10017947", "synonyms": ["diarrhea", "diarrhoea", "loose stools", "frequent loose stools", "watery stools", "bowel looseness"]},
    {"pt": "Constipation", "pt_code": "10010774", "llt": "Constipation", "llt_code": "10010774", "hlt": "Gastrointestinal atonic and hypomotility disorders NEC", "hlt_code": "10017950", "hlgt": "Gastrointestinal motility and defaecation conditions", "hlgt_code": "10017983", "soc": "Gastrointestinal disorders", "soc_code": "10017947", "synonyms": ["constipation", "irregular bowel movements", "hard stools", "costive", "fecal impaction"]},
    {"pt": "Abdominal pain", "pt_code": "10000081", "llt": "Abdominal pain", "llt_code": "10000081", "hlt": "Gastrointestinal and abdominal pains (excl oral and throat)", "hlt_code": "10017961", "hlgt": "Gastrointestinal signs and symptoms", "hlgt_code": "10017997", "soc": "Gastrointestinal disorders", "soc_code": "10017947", "synonyms": ["abdominal pain", "stomach ache", "belly pain", "abdominal cramps", "tummy ache", "gastric pain", "epigastric pain", "abd pain"]},
    {"pt": "Dyspepsia", "pt_code": "10013946", "llt": "Indigestion", "llt_code": "10021655", "hlt": "Dyspeptic signs and symptoms", "hlt_code": "10013948", "hlgt": "Gastrointestinal signs and symptoms", "hlgt_code": "10017997", "soc": "Gastrointestinal disorders", "soc_code": "10017947", "synonyms": ["dyspepsia", "indigestion", "heartburn", "acid reflux", "sour stomach", "gerd symptoms"]},
    {"pt": "Stomatitis", "pt_code": "10042128", "llt": "Mouth ulcers", "llt_code": "10028041", "hlt": "Stomatitis and ulceration", "hlt_code": "10042131", "hlgt": "Oral soft tissue conditions", "hlgt_code": "10030999", "soc": "Gastrointestinal disorders", "soc_code": "10017947", "synonyms": ["stomatitis", "oral mucositis", "mouth sores", "canker sore", "oral ulcers", "mouth ulcers"]},

    # Nervous System Disorders
    {"pt": "Headache", "pt_code": "10019211", "llt": "Headache", "llt_code": "10019211", "hlt": "Headaches NEC", "hlt_code": "10019231", "hlgt": "Headaches", "hlgt_code": "10019233", "soc": "Nervous system disorders", "soc_code": "10029205", "synonyms": ["headache", "cephalalgia", "migraine", "head pain", "tension headache", "throbbing headache", "cephalea"]},
    {"pt": "Dizziness", "pt_code": "10013573", "llt": "Dizziness", "llt_code": "10013573", "hlt": "Neurological signs and symptoms NEC", "hlt_code": "10029305", "hlgt": "Neurological disorders NEC", "hlgt_code": "10029295", "soc": "Nervous system disorders", "soc_code": "10029205", "synonyms": ["dizziness", "lightheadedness", "lightheaded", "giddiness", "woozy", "unsteady"]},
    {"pt": "Peripheral neuropathy", "pt_code": "10034607", "llt": "Peripheral neuropathy", "llt_code": "10034607", "hlt": "Peripheral neuropathies NEC", "hlt_code": "10034609", "hlgt": "Peripheral neuropathies", "hlgt_code": "10034610", "soc": "Nervous system disorders", "soc_code": "10029205", "synonyms": ["peripheral neuropathy", "numbness and tingling", "pins and needles", "paresthesia", "neuropathy", "tingling in fingers", "numb extremities"]},
    {"pt": "Somnolence", "pt_code": "10041349", "llt": "Drowsiness", "llt_code": "10013649", "hlt": "Disturbances in consciousness NEC", "hlt_code": "10013444", "hlgt": "Neurological disorders NEC", "hlgt_code": "10029295", "soc": "Nervous system disorders", "soc_code": "10029205", "synonyms": ["somnolence", "drowsiness", "excessive sleepiness", "sedation", "sleepy", "somnolent"]},
    {"pt": "Tremor", "pt_code": "10044565", "llt": "Tremor", "llt_code": "10044565", "hlt": "Tremor (excl congenital)", "hlt_code": "10044567", "hlgt": "Movement disorders (incl parkinsonism)", "hlgt_code": "10028045", "soc": "Nervous system disorders", "soc_code": "10029205", "synonyms": ["tremor", "hand tremor", "shaky hands", "trembling", "intentional tremor"]},
    {"pt": "Syncope", "pt_code": "10042772", "llt": "Fainting", "llt_code": "10016147", "hlt": "Transient signs and symptoms of nervous system", "hlt_code": "10044390", "hlgt": "Neurological disorders NEC", "hlgt_code": "10029295", "soc": "Nervous system disorders", "soc_code": "10029205", "synonyms": ["syncope", "fainting", "passed out", "blackout", "vasovagal syncope", "loss of consciousness"]},

    # Respiratory, Thoracic and Mediastinal Disorders
    {"pt": "Cough", "pt_code": "10011224", "llt": "Cough", "llt_code": "10011224", "hlt": "Coughing and associated symptoms", "hlt_code": "10011234", "hlgt": "Respiratory disorders NEC", "hlgt_code": "10038716", "soc": "Respiratory, thoracic and mediastinal disorders", "soc_code": "10038738", "synonyms": ["cough", "coughing", "dry cough", "productive cough", "chronic cough", "hacking cough"]},
    {"pt": "Dyspnoea", "pt_code": "10013968", "llt": "Shortness of breath", "llt_code": "10040604", "hlt": "Dyspnoeic conditions", "hlt_code": "10013963", "hlgt": "Respiratory disorders NEC", "hlgt_code": "10038716", "soc": "Respiratory, thoracic and mediastinal disorders", "soc_code": "10038738", "synonyms": ["dyspnea", "dyspnoea", "shortness of breath", "sob", "breathlessness", "difficulty breathing", "air hunger"]},
    {"pt": "Pneumonitis", "pt_code": "10035742", "llt": "Pneumonitis", "llt_code": "10035742", "hlt": "Pneumonitis (excl infective)", "hlt_code": "10035743", "hlgt": "Lower respiratory tract disorders (excl obstruction and infection)", "hlgt_code": "10024967", "soc": "Respiratory, thoracic and mediastinal disorders", "soc_code": "10038738", "synonyms": ["pneumonitis", "immune-mediated pneumonitis", "lung inflammation", "interstitial lung disease", "radiation pneumonitis"]},
    {"pt": "Epistaxis", "pt_code": "10015090", "llt": "Nosebleed", "llt_code": "10029780", "hlt": "Nasal disorders NEC", "hlt_code": "10028751", "hlgt": "Upper respiratory tract disorders (excl infections)", "hlgt_code": "10046303", "soc": "Respiratory, thoracic and mediastinal disorders", "soc_code": "10038738", "synonyms": ["epistaxis", "nose bleed", "nosebleed", "nasal bleeding", "bloody nose"]},

    # Skin and Subcutaneous Tissue Disorders
    {"pt": "Rash", "pt_code": "10037844", "llt": "Skin rash", "llt_code": "10040914", "hlt": "Rashes, eruptions and exanthems NEC", "hlt_code": "10037868", "hlgt": "Epidermal and dermal conditions", "hlgt_code": "10014982", "soc": "Skin and subcutaneous tissue disorders", "soc_code": "10040785", "synonyms": ["rash", "skin rash", "erythematous rash", "maculopapular rash", "skin eruption", "exanthem"]},
    {"pt": "Pruritus", "pt_code": "10037087", "llt": "Itching", "llt_code": "10023084", "hlt": "Pruritus NEC", "hlt_code": "10037088", "hlgt": "Epidermal and dermal conditions", "hlgt_code": "10014982", "soc": "Skin and subcutaneous tissue disorders", "soc_code": "10040785", "synonyms": ["pruritus", "itching", "itchy skin", "itchiness", "scratching"]},
    {"pt": "Alopecia", "pt_code": "10001760", "llt": "Hair loss", "llt_code": "10019012", "hlt": "Alopecias", "hlt_code": "10001761", "hlgt": "Skin appendage conditions", "hlgt_code": "10040776", "soc": "Skin and subcutaneous tissue disorders", "soc_code": "10040785", "synonyms": ["alopecia", "hair loss", "hair thinning", "falling hair", "balding"]},
    {"pt": "Dry skin", "pt_code": "10013786", "llt": "Dry skin", "llt_code": "10013786", "hlt": "Epidermal conditions NEC", "hlt_code": "10014980", "hlgt": "Epidermal and dermal conditions", "hlgt_code": "10014982", "soc": "Skin and subcutaneous tissue disorders", "soc_code": "10040785", "synonyms": ["dry skin", "xerosis", "flaky skin", "skin dryness", "xeroderma"]},

    # Cardiac Disorders
    {"pt": "Atrial fibrillation", "pt_code": "10003658", "llt": "Atrial fibrillation", "llt_code": "10003658", "hlt": "Supraventricular arrhythmias", "hlt_code": "10042600", "hlgt": "Cardiac arrhythmias", "hlgt_code": "10007521", "soc": "Cardiac disorders", "soc_code": "10007541", "synonyms": ["atrial fibrillation", "afib", "a-fib", "af", "auricular fibrillation", "atrial flutter-fibrillation"]},
    {"pt": "Myocardial infarction", "pt_code": "10028596", "llt": "Heart attack", "llt_code": "10019253", "hlt": "Ischaemic coronary artery disorders", "hlt_code": "10022998", "hlgt": "Coronary artery disorders", "hlgt_code": "10011078", "soc": "Cardiac disorders", "soc_code": "10007541", "synonyms": ["myocardial infarction", "mi", "heart attack", "acute mi", "nstemi", "stemi", "coronary thrombosis"]},
    {"pt": "Palpitations", "pt_code": "10033557", "llt": "Palpitations", "llt_code": "10033557", "hlt": "Cardiac signs and symptoms NEC", "hlt_code": "10007577", "hlgt": "Cardiac disorders NEC", "hlgt_code": "10007540", "soc": "Cardiac disorders", "soc_code": "10007541", "synonyms": ["palpitations", "racing heart", "heart pounding", "fluttering heart", "rapid heartbeat"]},
    {"pt": "Cardiac failure congestive", "pt_code": "10007556", "llt": "Congestive heart failure", "llt_code": "10010645", "hlt": "Heart failures NEC", "hlt_code": "10019277", "hlgt": "Heart failures", "hlgt_code": "10019276", "soc": "Cardiac disorders", "soc_code": "10007541", "synonyms": ["congestive heart failure", "chf", "heart failure", "cardiac failure", "acute decompensated heart failure"]},

    # Vascular Disorders
    {"pt": "Hypertension", "pt_code": "10020772", "llt": "High blood pressure", "llt_code": "10020063", "hlt": "Vascular hypertensive disorders NEC", "hlt_code": "10047069", "hlgt": "Vascular hypertensive disorders", "hlgt_code": "10047068", "soc": "Vascular disorders", "soc_code": "10047065", "synonyms": ["hypertension", "high blood pressure", "elevated bp", "htn", "essential hypertension", "hypertensive crisis"]},
    {"pt": "Hypotension", "pt_code": "10021097", "llt": "Low blood pressure", "llt_code": "10024929", "hlt": "Vascular hypotensive disorders", "hlt_code": "10047077", "hlgt": "Decreased and nonspecific blood pressure disorders and shock", "hlgt_code": "10012170", "soc": "Vascular disorders", "soc_code": "10047065", "synonyms": ["hypotension", "low blood pressure", "low bp", "orthostatic hypotension", "postural hypotension"]},
    {"pt": "Deep vein thrombosis", "pt_code": "10012144", "llt": "Deep vein thrombosis", "llt_code": "10012144", "hlt": "Peripheral embolism and thrombosis", "hlt_code": "10034574", "hlgt": "Embolism and thrombosis", "hlgt_code": "10014569", "soc": "Vascular disorders", "soc_code": "10047065", "synonyms": ["deep vein thrombosis", "dvt", "venous thromboembolism", "vte", "deep venous thrombosis"]},

    # Blood and Lymphatic System Disorders
    {"pt": "Anaemia", "pt_code": "10002034", "llt": "Anemia", "llt_code": "10002272", "hlt": "Anaemias NEC", "hlt_code": "10002035", "hlgt": "Anaemias nonhaemolytic and MCA", "hlgt_code": "10002036", "soc": "Blood and lymphatic system disorders", "soc_code": "10005329", "synonyms": ["anemia", "anaemia", "low hemoglobin", "low hgb", "decreased red blood cells", "iron deficiency anemia"]},
    {"pt": "Neutropenia", "pt_code": "10029354", "llt": "Neutropenia", "llt_code": "10029354", "hlt": "Neutropenias", "hlt_code": "10029355", "hlgt": "White blood cell disorders", "hlgt_code": "10047942", "soc": "Blood and lymphatic system disorders", "soc_code": "10005329", "synonyms": ["neutropenia", "low anc", "absolute neutrophil count decreased", "febrile neutropenia", "agranulocytosis"]},
    {"pt": "Thrombocytopenia", "pt_code": "10043554", "llt": "Low platelets", "llt_code": "10024976", "hlt": "Thrombocytopenias", "hlt_code": "10043555", "hlgt": "Platelet disorders", "hlgt_code": "10035528", "soc": "Blood and lymphatic system disorders", "soc_code": "10005329", "synonyms": ["thrombocytopenia", "low platelets", "platelet count decreased", "thrombopenia"]},
    {"pt": "Leukopenia", "pt_code": "10024384", "llt": "Low white blood count", "llt_code": "10025000", "hlt": "Leukopenias NEC", "hlt_code": "10024385", "hlgt": "White blood cell disorders", "hlgt_code": "10047942", "soc": "Blood and lymphatic system disorders", "soc_code": "10005329", "synonyms": ["leukopenia", "low wbc", "white blood cell count decreased", "leucopenia"]},

    # Infections and Infestations
    {"pt": "Upper respiratory tract infection", "pt_code": "10046306", "llt": "Cold", "llt_code": "10009995", "hlt": "Upper respiratory tract infections NEC", "hlt_code": "10046307", "hlgt": "Infections - pathogen unspecified", "hlgt_code": "10021881", "soc": "Infections and infestations", "soc_code": "10021881", "synonyms": ["urti", "upper respiratory tract infection", "common cold", "head cold", "rhinopharyngitis", "nasopharyngitis"]},
    {"pt": "Urinary tract infection", "pt_code": "10046571", "llt": "UTI", "llt_code": "10046554", "hlt": "Urinary tract infections", "hlt_code": "10046573", "hlgt": "Infections - pathogen unspecified", "hlgt_code": "10021881", "soc": "Infections and infestations", "soc_code": "10021881", "synonyms": ["uti", "urinary tract infection", "bladder infection", "cystitis", "pyelonephritis"]},
    {"pt": "Pneumonia", "pt_code": "10035664", "llt": "Pneumonia", "llt_code": "10035664", "hlt": "Lower respiratory tract and lung infections", "hlt_code": "10024969", "hlgt": "Infections - pathogen unspecified", "hlgt_code": "10021881", "soc": "Infections and infestations", "soc_code": "10021881", "synonyms": ["pneumonia", "lung infection", "bronchopneumonia", "community acquired pneumonia", "cap", "bacterial pneumonia"]},
    {"pt": "COVID-19", "pt_code": "10084268", "llt": "COVID-19 infection", "llt_code": "10084272", "hlt": "Coronavirus infections", "hlt_code": "10051187", "hlgt": "Viral infectious disorders", "hlgt_code": "10047461", "soc": "Infections and infestations", "soc_code": "10021881", "synonyms": ["covid-19", "covid", "sars-cov-2", "coronavirus", "novel coronavirus infection"]},
    {"pt": "Sepsis", "pt_code": "10040047", "llt": "Sepsis", "llt_code": "10040047", "hlt": "Sepsis, bacteraemia and viraemia NEC", "hlt_code": "10040050", "hlgt": "Infections - pathogen unspecified", "hlgt_code": "10021881", "soc": "Infections and infestations", "soc_code": "10021881", "synonyms": ["sepsis", "septicemia", "septic shock", "bloodstream infection", "bacteremia"]},

    # Investigations / Laboratory
    {"pt": "Alanine aminotransferase increased", "pt_code": "10001551", "llt": "ALT increased", "llt_code": "10001550", "hlt": "Liver function analyses", "hlt_code": "10024690", "hlgt": "Hepatobiliary investigations", "hlgt_code": "10019808", "soc": "Investigations", "soc_code": "10022891", "synonyms": ["alt increased", "elevated alt", "sgpt increased", "alanine aminotransferase elevation", "transaminitis"]},
    {"pt": "Aspartate aminotransferase increased", "pt_code": "10003481", "llt": "AST increased", "llt_code": "10003480", "hlt": "Liver function analyses", "hlt_code": "10024690", "hlgt": "Hepatobiliary investigations", "hlgt_code": "10019808", "soc": "Investigations", "soc_code": "10022891", "synonyms": ["ast increased", "elevated ast", "sgot increased", "aspartate aminotransferase elevation"]},
    {"pt": "Blood creatinine increased", "pt_code": "10005483", "llt": "Creatinine elevated", "llt_code": "10011370", "hlt": "Renal function analyses", "hlt_code": "10038536", "hlgt": "Renal and urinary tract investigations", "hlgt_code": "10038537", "soc": "Investigations", "soc_code": "10022891", "synonyms": ["creatinine increased", "elevated serum creatinine", "high creatinine", "renal impairment lab", "cr elevated"]},
    {"pt": "Blood bilirubin increased", "pt_code": "10005364", "llt": "Hyperbilirubinemia", "llt_code": "10020642", "hlt": "Bilirubin analyses", "hlt_code": "10004677", "hlgt": "Hepatobiliary investigations", "hlgt_code": "10019808", "soc": "Investigations", "soc_code": "10022891", "synonyms": ["bilirubin increased", "hyperbilirubinemia", "elevated bilirubin", "jaundice lab"]},
    {"pt": "Weight decreased", "pt_code": "10047895", "llt": "Weight loss", "llt_code": "10047898", "hlt": "Physical examination procedures and organ system status", "hlt_code": "10034988", "hlgt": "Physical examination and organ system status topics", "hlgt_code": "10034991", "soc": "Investigations", "soc_code": "10022891", "synonyms": ["weight loss", "weight decreased", "loss of weight", "unintentional weight loss"]},

    # Metabolism and Nutrition Disorders
    {"pt": "Decreased appetite", "pt_code": "10061428", "llt": "Anorexia", "llt_code": "10002646", "hlt": "Appetite disorders", "hlt_code": "10003046", "hlgt": "Appetite and general nutritional disorders", "hlgt_code": "10003047", "soc": "Metabolism and nutrition disorders", "soc_code": "10027433", "synonyms": ["decreased appetite", "loss of appetite", "anorexia", "poor appetite", "not eating well"]},
    {"pt": "Hyperglycaemia", "pt_code": "10020635", "llt": "Hyperglycemia", "llt_code": "10020635", "hlt": "Hyperglycaemic conditions NEC", "hlt_code": "10020638", "hlgt": "Carbohydrate metabolism disorders", "hlgt_code": "10007204", "soc": "Metabolism and nutrition disorders", "soc_code": "10027433", "synonyms": ["hyperglycemia", "hyperglycaemia", "high blood sugar", "elevated glucose", "high glucose"]},
    {"pt": "Hypoglycaemia", "pt_code": "10020993", "llt": "Hypoglycemia", "llt_code": "10020993", "hlt": "Hypoglycaemic conditions NEC", "hlt_code": "10020997", "hlgt": "Carbohydrate metabolism disorders", "hlgt_code": "10007204", "soc": "Metabolism and nutrition disorders", "soc_code": "10027433", "synonyms": ["hypoglycemia", "hypoglycaemia", "low blood sugar", "low blood glucose", "hypo"]},
    {"pt": "Hypokalaemia", "pt_code": "10021015", "llt": "Hypokalemia", "llt_code": "10021015", "hlt": "Potassium imbalance", "hlt_code": "10036399", "hlgt": "Electrolyte and fluid balance conditions", "hlgt_code": "10014389", "soc": "Metabolism and nutrition disorders", "soc_code": "10027433", "synonyms": ["hypokalemia", "hypokalaemia", "low potassium", "low serum potassium"]},
    {"pt": "Hyponatraemia", "pt_code": "10021038", "llt": "Hyponatremia", "llt_code": "10021038", "hlt": "Sodium imbalance", "hlt_code": "10041221", "hlgt": "Electrolyte and fluid balance conditions", "hlgt_code": "10014389", "soc": "Metabolism and nutrition disorders", "soc_code": "10027433", "synonyms": ["hyponatremia", "hyponatraemia", "low sodium", "low serum sodium"]},

    # Psychiatric Disorders
    {"pt": "Insomnia", "pt_code": "10022437", "llt": "Sleeplessness", "llt_code": "10040984", "hlt": "Disturbances in initiating and maintaining sleep", "hlt_code": "10013448", "hlgt": "Sleep disorders and disturbances", "hlgt_code": "10040989", "soc": "Psychiatric disorders", "soc_code": "10037175", "synonyms": ["insomnia", "sleeplessness", "trouble sleeping", "difficulty sleeping", "sleep disruption", "poor sleep"]},
    {"pt": "Anxiety", "pt_code": "10002855", "llt": "Anxiety", "llt_code": "10002855", "hlt": "Anxiety symptoms", "hlt_code": "10002863", "hlgt": "Anxiety disorders and symptoms", "hlgt_code": "10002860", "soc": "Psychiatric disorders", "soc_code": "10037175", "synonyms": ["anxiety", "nervousness", "feeling anxious", "panic", "generalized anxiety", "anxiousness"]},
    {"pt": "Depression", "pt_code": "10012378", "llt": "Depression", "llt_code": "10012378", "hlt": "Depressive disorders", "hlt_code": "10012397", "hlgt": "Depressed mood disorders and disturbances", "hlgt_code": "10012374", "soc": "Psychiatric disorders", "soc_code": "10037175", "synonyms": ["depression", "depressed mood", "feeling low", "major depression", "sadness", "clinical depression"]}
]


# Standard WHO Drug Global Curated Knowledge Base (Concomitant Medications, Active Ingredients, ATC Hierarchy)
WHO_DRUG_KNOWLEDGE_BASE: List[Dict[str, Any]] = [
    # Analgesics & Antipyretics
    {
        "preferred_name": "PARACETAMOL", "drug_record_number": "00049001", "therapeutic_class": "OTHER ANALGESICS AND ANTIPYRETICS",
        "atc_code": "N02BE01", "atc_level1": "N: NERVOUS SYSTEM", "atc_level2": "N02: ANALGESICS", "atc_level3": "N02B: OTHER ANALGESICS AND ANTIPYRETICS", "atc_level4": "N02BE: ANILIDES",
        "synonyms": ["paracetamol", "acetaminophen", "tylenol", "panadol", "apap", "calpol", "acetaminophen 500mg", "tylenol extra strength"]
    },
    {
        "preferred_name": "IBUPROFEN", "drug_record_number": "00021001", "therapeutic_class": "NON-STEROIDAL ANTI-INFLAMMATORY AGENTS",
        "atc_code": "M01AE01", "atc_level1": "M: MUSCULO-SKELETAL SYSTEM", "atc_level2": "M01: ANTIINFLAMMATORY AND ANTIRHEUMATIC PRODUCTS", "atc_level3": "M01A: ANTIINFLAMMATORY AND ANTIRHEUMATIC PRODUCTS, NON-STEROIDS", "atc_level4": "M01AE: PROPIONIC ACID DERIVATIVES",
        "synonyms": ["ibuprofen", "advil", "motrin", "nurofen", "brufen", "ibuprofen 400mg", "motrin ib", "advil liquid gels"]
    },
    {
        "preferred_name": "ASPIRIN", "drug_record_number": "00003001", "therapeutic_class": "PLATELET AGGREGATION INHIBITORS EXCL. HEPARIN",
        "atc_code": "B01AC06", "atc_level1": "B: BLOOD AND BLOOD FORMING ORGANS", "atc_level2": "B01: ANTITHROMBOTIC AGENTS", "atc_level3": "B01A: ANTITHROMBOTIC AGENTS", "atc_level4": "B01AC: PLATELET AGGREGATION INHIBITORS EXCL. HEPARIN",
        "synonyms": ["aspirin", "acetylsalicylic acid", "bayer aspirin", "ecotrin", "asa", "baby aspirin", "aspirin 81mg"]
    },
    {
        "preferred_name": "MORPHINE", "drug_record_number": "00018001", "therapeutic_class": "NATURAL OPIUM ALKALOIDS",
        "atc_code": "N02AA01", "atc_level1": "N: NERVOUS SYSTEM", "atc_level2": "N02: ANALGESICS", "atc_level3": "N02A: OPIOIDS", "atc_level4": "N02AA: NATURAL OPIUM ALKALOIDS",
        "synonyms": ["morphine", "morphine sulfate", "ms contin", "kadian", "oramorph", "morphine iv"]
    },
    {
        "preferred_name": "OXYCODONE", "drug_record_number": "00067001", "therapeutic_class": "NATURAL OPIUM ALKALOIDS",
        "atc_code": "N02AA05", "atc_level1": "N: NERVOUS SYSTEM", "atc_level2": "N02: ANALGESICS", "atc_level3": "N02A: OPIOIDS", "atc_level4": "N02AA: NATURAL OPIUM ALKALOIDS",
        "synonyms": ["oxycodone", "oxycontin", "roxicodone", "percocet", "oxycodone hcl"]
    },

    # Cardiovascular & Antihypertensives
    {
        "preferred_name": "AMLODIPINE", "drug_record_number": "00312001", "therapeutic_class": "SELECTIVE CALCIUM CHANNEL BLOCKERS WITH MAINLY VASCULAR EFFECTS",
        "atc_code": "C08CA01", "atc_level1": "C: CARDIOVASCULAR SYSTEM", "atc_level2": "C08: CALCIUM CHANNEL BLOCKERS", "atc_level3": "C08C: SELECTIVE CALCIUM CHANNEL BLOCKERS WITH MAINLY VASCULAR EFFECTS", "atc_level4": "C08CA: DIHYDROPYRIDINE DERIVATIVES",
        "synonyms": ["amlodipine", "norvasc", "amlodipine besylate", "amlodipine 5mg", "amlodipine 10mg"]
    },
    {
        "preferred_name": "LISINOPRIL", "drug_record_number": "00289001", "therapeutic_class": "ACE INHIBITORS, PLAIN",
        "atc_code": "C09AA03", "atc_level1": "C: CARDIOVASCULAR SYSTEM", "atc_level2": "C09: AGENTS ACTING ON THE RENIN-ANGIOTENSIN SYSTEM", "atc_level3": "C09A: ACE INHIBITORS, PLAIN", "atc_level4": "C09AA: ACE INHIBITORS, PLAIN",
        "synonyms": ["lisinopril", "zestril", "prinivil", "lisinopril 10mg", "lisinopril 20mg"]
    },
    {
        "preferred_name": "LOSARTAN", "drug_record_number": "00543001", "therapeutic_class": "ANGIOTENSIN II RECEPTOR BLOCKERS (ARBS), PLAIN",
        "atc_code": "C09CA01", "atc_level1": "C: CARDIOVASCULAR SYSTEM", "atc_level2": "C09: AGENTS ACTING ON THE RENIN-ANGIOTENSIN SYSTEM", "atc_level3": "C09C: ANGIOTENSIN II RECEPTOR BLOCKERS (ARBS), PLAIN", "atc_level4": "C09CA: ANGIOTENSIN II RECEPTOR BLOCKERS (ARBS), PLAIN",
        "synonyms": ["losartan", "cozaar", "losartan potassium", "losartan 50mg", "losartan 100mg"]
    },
    {
        "preferred_name": "ATORVASTATIN", "drug_record_number": "00612001", "therapeutic_class": "HMG COA REDUCTASE INHIBITORS",
        "atc_code": "C10AA05", "atc_level1": "C: CARDIOVASCULAR SYSTEM", "atc_level2": "C10: LIPID MODIFYING AGENTS", "atc_level3": "C10A: LIPID MODIFYING AGENTS, PLAIN", "atc_level4": "C10AA: HMG COA REDUCTASE INHIBITORS",
        "synonyms": ["atorvastatin", "lipitor", "atorvastatin calcium", "atorvastatin 20mg", "atorvastatin 40mg", "lipitor 10mg"]
    },
    {
        "preferred_name": "METOPROLOL", "drug_record_number": "00145001", "therapeutic_class": "BETA BLOCKING AGENTS, SELECTIVE",
        "atc_code": "C07AB02", "atc_level1": "C: CARDIOVASCULAR SYSTEM", "atc_level2": "C07: BETA BLOCKING AGENTS", "atc_level3": "C07A: BETA BLOCKING AGENTS", "atc_level4": "C07AB: BETA BLOCKING AGENTS, SELECTIVE",
        "synonyms": ["metoprolol", "lopressor", "toprol xl", "metoprolol succinate", "metoprolol tartrate", "toprol"]
    },
    {
        "preferred_name": "FUROSEMIDE", "drug_record_number": "00034001", "therapeutic_class": "HIGH-CEILING DIURETICS",
        "atc_code": "C03CA01", "atc_level1": "C: CARDIOVASCULAR SYSTEM", "atc_level2": "C03: DIURETICS", "atc_level3": "C03C: HIGH-CEILING DIURETICS", "atc_level4": "C03CA: SULFONAMIDES, PLAIN",
        "synonyms": ["furosemide", "lasix", "furosemide 40mg", "lasix 20mg", "frusemide"]
    },

    # Antidiabetics & Metabolism
    {
        "preferred_name": "METFORMIN", "drug_record_number": "00088001", "therapeutic_class": "BIGUANIDES",
        "atc_code": "A10BA02", "atc_level1": "A: ALIMENTARY TRACT AND METABOLISM", "atc_level2": "A10: DRUGS USED IN DIABETES", "atc_level3": "A10B: BLOOD GLUCOSE LOWERING DRUGS, EXCL. INSULINS", "atc_level4": "A10BA: BIGUANIDES",
        "synonyms": ["metformin", "glucophage", "metformin hcl", "glucophage xr", "metformin 500mg", "metformin 1000mg"]
    },
    {
        "preferred_name": "SEMAGLUTIDE", "drug_record_number": "01239001", "therapeutic_class": "GLUCAGON-LIKE PEPTIDE-1 (GLP-1) RECEPTOR AGONISTS",
        "atc_code": "A10BJ06", "atc_level1": "A: ALIMENTARY TRACT AND METABOLISM", "atc_level2": "A10: DRUGS USED IN DIABETES", "atc_level3": "A10B: BLOOD GLUCOSE LOWERING DRUGS, EXCL. INSULINS", "atc_level4": "A10BJ: GLUCAGON-LIKE PEPTIDE-1 RECEPTOR AGONISTS",
        "synonyms": ["semaglutide", "ozempic", "wegovy", "rybelsus", "semaglutide subq"]
    },
    {
        "preferred_name": "EMPAGLIFLOZIN", "drug_record_number": "01124001", "therapeutic_class": "SODIUM-GLUCOSE CO-TRANSPORTER 2 (SGLT2) INHIBITORS",
        "atc_code": "A10BK03", "atc_level1": "A: ALIMENTARY TRACT AND METABOLISM", "atc_level2": "A10: DRUGS USED IN DIABETES", "atc_level3": "A10B: BLOOD GLUCOSE LOWERING DRUGS, EXCL. INSULINS", "atc_level4": "A10BK: SODIUM-GLUCOSE CO-TRANSPORTER 2 (SGLT2) INHIBITORS",
        "synonyms": ["empagliflozin", "jardiance", "empagliflozin 10mg", "empagliflozin 25mg"]
    },
    {
        "preferred_name": "INSULIN GLARGINE", "drug_record_number": "00782001", "therapeutic_class": "INSULINS AND ANALOGUES FOR INJECTION, LONG-ACTING",
        "atc_code": "A10AE04", "atc_level1": "A: ALIMENTARY TRACT AND METABOLISM", "atc_level2": "A10: DRUGS USED IN DIABETES", "atc_level3": "A10A: INSULINS AND ANALOGUES", "atc_level4": "A10AE: INSULINS AND ANALOGUES FOR INJECTION, LONG-ACTING",
        "synonyms": ["insulin glargine", "lantus", "toujeo", "basaglar", "glargine"]
    },

    # Anti-Infectives & Antibiotics
    {
        "preferred_name": "AMOXICILLIN", "drug_record_number": "00055001", "therapeutic_class": "PENICILLINS WITH EXTENDED SPECTRUM",
        "atc_code": "J01CA04", "atc_level1": "J: ANTIINFECTIVES FOR SYSTEMIC USE", "atc_level2": "J01: ANTIBACTERIALS FOR SYSTEMIC USE", "atc_level3": "J01C: BETA-LACTAM ANTIBACTERIALS, PENICILLINS", "atc_level4": "J01CA: PENICILLINS WITH EXTENDED SPECTRUM",
        "synonyms": ["amoxicillin", "amoxil", "amoxicillin/clavulanate", "augmentin", "amoxicillin 500mg"]
    },
    {
        "preferred_name": "AZITHROMYCIN", "drug_record_number": "00388001", "therapeutic_class": "MACROLIDES",
        "atc_code": "J01FA10", "atc_level1": "J: ANTIINFECTIVES FOR SYSTEMIC USE", "atc_level2": "J01: ANTIBACTERIALS FOR SYSTEMIC USE", "atc_level3": "J01F: MACROLIDES, LINCOSAMIDES AND STREPTOGRAMINS", "atc_level4": "J01FA: MACROLIDES",
        "synonyms": ["azithromycin", "zithromax", "z-pak", "azithromycin 250mg", "zmax"]
    },
    {
        "preferred_name": "CIPROFLOXACIN", "drug_record_number": "00256001", "therapeutic_class": "FLUOROQUINOLONES",
        "atc_code": "J01MA02", "atc_level1": "J: ANTIINFECTIVES FOR SYSTEMIC USE", "atc_level2": "J01: ANTIBACTERIALS FOR SYSTEMIC USE", "atc_level3": "J01M: QUINOLONE ANTIBACTERIALS", "atc_level4": "J01MA: FLUOROQUINOLONES",
        "synonyms": ["ciprofloxacin", "cipro", "ciprofloxacin hcl", "cipro 500mg"]
    },

    # Oncology & Immunomodulators
    {
        "preferred_name": "PEMBROLIZUMAB", "drug_record_number": "01155001", "therapeutic_class": "MONOCLONAL ANTIBODIES AND ANTIBODY DRUG CONJUGATES",
        "atc_code": "L01FF02", "atc_level1": "L: ANTINEOPLASTIC AND IMMUNOMODULATING AGENTS", "atc_level2": "L01: ANTINEOPLASTIC AGENTS", "atc_level3": "L01F: MONOCLONAL ANTIBODIES AND ANTIBODY DRUG CONJUGATES", "atc_level4": "L01FF: PD-1/PD-L1 (PROGRAMMED CELL DEATH PROTEIN 1/PD-L1) INHIBITORS",
        "synonyms": ["pembrolizumab", "keytruda", "pembro", "mk-3475", "keytruda iv"]
    },
    {
        "preferred_name": "NIVOLUMAB", "drug_record_number": "01148001", "therapeutic_class": "MONOCLONAL ANTIBODIES AND ANTIBODY DRUG CONJUGATES",
        "atc_code": "L01FF01", "atc_level1": "L: ANTINEOPLASTIC AND IMMUNOMODULATING AGENTS", "atc_level2": "L01: ANTINEOPLASTIC AGENTS", "atc_level3": "L01F: MONOCLONAL ANTIBODIES AND ANTIBODY DRUG CONJUGATES", "atc_level4": "L01FF: PD-1/PD-L1 (PROGRAMMED CELL DEATH PROTEIN 1/PD-L1) INHIBITORS",
        "synonyms": ["nivolumab", "opdivo", "nivo", "bms-936558", "opdivo infusion"]
    },
    {
        "preferred_name": "TRASTUZUMAB", "drug_record_number": "00684001", "therapeutic_class": "HER2 (HUMAN EPIDERMAL GROWTH FACTOR RECEPTOR 2) INHIBITORS",
        "atc_code": "L01FD01", "atc_level1": "L: ANTINEOPLASTIC AND IMMUNOMODULATING AGENTS", "atc_level2": "L01: ANTINEOPLASTIC AGENTS", "atc_level3": "L01F: MONOCLONAL ANTIBODIES AND ANTIBODY DRUG CONJUGATES", "atc_level4": "L01FD: HER2 (HUMAN EPIDERMAL GROWTH FACTOR RECEPTOR 2) INHIBITORS",
        "synonyms": ["trastuzumab", "herceptin", "herceptin iv", "trastuzumab-dkst", "ogivri"]
    },
    {
        "preferred_name": "PACLITAXEL", "drug_record_number": "00450001", "therapeutic_class": "TAXANES",
        "atc_code": "L01CD01", "atc_level1": "L: ANTINEOPLASTIC AND IMMUNOMODULATING AGENTS", "atc_level2": "L01: ANTINEOPLASTIC AGENTS", "atc_level3": "L01C: PLANT ALKALOIDS AND OTHER NATURAL PRODUCTS", "atc_level4": "L01CD: TAXANES",
        "synonyms": ["paclitaxel", "taxol", "abraxane", "nab-paclitaxel", "paclitaxel iv"]
    },

    # Gastrointestinal & Antiemetics
    {
        "preferred_name": "OMEPRAZOLE", "drug_record_number": "00305001", "therapeutic_class": "PROTON PUMP INHIBITORS",
        "atc_code": "A02BC01", "atc_level1": "A: ALIMENTARY TRACT AND METABOLISM", "atc_level2": "A02: DRUGS FOR ACID RELATED DISORDERS", "atc_level3": "A02B: DRUGS FOR PEPTIC ULCER AND GASTRO-OESOPHAGEAL REFLUX DISEASE (GORD)", "atc_level4": "A02BC: PROTON PUMP INHIBITORS",
        "synonyms": ["omeprazole", "prilosec", "prilosec otc", "losec", "omeprazole 20mg"]
    },
    {
        "preferred_name": "ONDANSETRON", "drug_record_number": "00401001", "therapeutic_class": "SEROTONIN (5HT3) ANTAGONISTS",
        "atc_code": "A04AA01", "atc_level1": "A: ALIMENTARY TRACT AND METABOLISM", "atc_level2": "A04: ANTIEMETICS AND ANTINAUSEANTS", "atc_level3": "A04A: ANTIEMETICS AND ANTINAUSEANTS", "atc_level4": "A04AA: SEROTONIN (5HT3) ANTAGONISTS",
        "synonyms": ["ondansetron", "zofran", "zofran odt", "ondansetron 4mg", "ondansetron 8mg", "zofran iv"]
    }
]


# ==================== STRING NORMALIZATION & DISTANCE UTILS ====================

def normalize_clinical_term(term: str) -> str:
    """Cleans punctuation, dosages, frequencies, and extra spaces from verbatim clinical terms."""
    if not term:
        return ""
    t = term.lower().strip()
    # Remove dosages e.g. "500mg", "10 mg", "0.5g", "100 mcg", "5ml"
    t = re.sub(r"\b\d+(\.\d+)?\s*(mg|mcg|g|ml|iu|units?|tablets?|caps?|pills?|%)\b", " ", t)
    # Remove administration routes & dosing frequencies e.g. "po", "iv", "sc", "subq", "oral", "daily", "bid", "tid", "qid", "prn", "q6h", "q12h", "q3w"
    t = re.sub(r"\b(po|iv|sc|subq|im|oral|topical|daily|bid|tid|qid|prn|q\d+h|q\d+w|q\d+d|qd|od)\b", " ", t)
    # Replace punctuation with spaces
    t = re.sub(r"[^\w\s]", " ", t)
    # Normalize multiple whitespace
    t = re.sub(r"\s+", " ", t).strip()
    return t


def levenshtein_similarity(s1: str, s2: str) -> float:
    """Calculates Levenshtein similarity ratio between 0.0 and 1.0."""
    if s1 == s2:
        return 1.0
    if not s1 or not s2:
        return 0.0
    
    len1, len2 = len(s1), len(s2)
    matrix = [[0] * (len2 + 1) for _ in range(len1 + 1)]
    for i in range(len1 + 1):
        matrix[i][0] = i
    for j in range(len2 + 1):
        matrix[0][j] = j
    
    for i in range(1, len1 + 1):
        for j in range(1, len2 + 1):
            cost = 0 if s1[i - 1] == s2[j - 1] else 1
            matrix[i][j] = min(
                matrix[i - 1][j] + 1,      # deletion
                matrix[i][j - 1] + 1,      # insertion
                matrix[i - 1][j - 1] + cost # substitution
            )
    
    dist = matrix[len1][len2]
    max_len = max(len1, len2)
    return max(0.0, 1.0 - (dist / max_len))


# ==================== CODER ENGINES ====================

class MedDRACoder:
    """Resolves verbatim reported terms to MedDRA 26.1 standard terms and hierarchies."""

    def __init__(self, custom_dictionary: Optional[List[Dict[str, Any]]] = None):
        self.dictionary = custom_dictionary or MEDDRA_KNOWLEDGE_BASE
        self._build_index()

    def _build_index(self):
        self.exact_map: Dict[str, Dict[str, Any]] = {}
        self.synonym_map: Dict[str, Dict[str, Any]] = {}
        for entry in self.dictionary:
            pt_key = entry["pt"].lower().strip()
            self.exact_map[pt_key] = entry
            llt_key = entry["llt"].lower().strip()
            self.exact_map[llt_key] = entry
            for syn in entry.get("synonyms", []):
                s_key = normalize_clinical_term(syn)
                if s_key:
                    self.synonym_map[s_key] = entry

    def code_term(self, verbatim: str) -> MedDRARecord:
        """Codes a single verbatim term through multi-tier matching."""
        if not verbatim or not verbatim.strip():
            return MedDRARecord(
                verbatim="", llt="", llt_code="", pt="", pt_code="",
                hlt="", hlt_code="", hlgt="", hlgt_code="", soc="", soc_code="",
                confidence=0.0, match_tier="UNCODED", needs_review=True
            )

        raw_clean = verbatim.strip()
        norm = normalize_clinical_term(raw_clean)

        # Tier 1: Exact Match on PT or LLT
        if raw_clean.lower() in self.exact_map:
            e = self.exact_map[raw_clean.lower()]
            return self._build_record(raw_clean, e, confidence=1.0, match_tier="EXACT_MATCH")

        # Tier 2: Normalized Synonym Match
        if norm in self.synonym_map:
            e = self.synonym_map[norm]
            return self._build_record(raw_clean, e, confidence=0.96, match_tier="SYNONYM_MATCH")

        # Tier 2b: Substring / Token Match in Synonym Dictionary
        for syn_key, entry in self.synonym_map.items():
            if len(syn_key) > 3 and (syn_key in norm or norm in syn_key):
                return self._build_record(raw_clean, entry, confidence=0.90, match_tier="SYNONYM_SUBSTRING")

        # Tier 3: Fuzzy Levenshtein Match against all PTs and Synonyms
        best_score = 0.0
        best_entry = None
        for syn_key, entry in self.synonym_map.items():
            score = levenshtein_similarity(norm, syn_key)
            if score > best_score:
                best_score = score
                best_entry = entry

        if best_score >= 0.75 and best_entry:
            return self._build_record(
                raw_clean, best_entry,
                confidence=round(best_score, 2),
                match_tier="FUZZY_MATCH",
                needs_review=(best_score < 0.85)
            )

        # Tier 4: Fallback Uncoded (flag for human review)
        return MedDRARecord(
            verbatim=raw_clean,
            llt=raw_clean.title(),
            llt_code="10099999",
            pt=raw_clean.title(),
            pt_code="10099999",
            hlt="Uncoded signs and symptoms",
            hlt_code="10099998",
            hlgt="Uncoded clinical conditions",
            hlgt_code="10099997",
            soc="General disorders and administration site conditions",
            soc_code="10018065",
            confidence=0.40,
            match_tier="FALLBACK_UNCODED",
            needs_review=True
        )

    def _build_record(self, verbatim: str, entry: Dict[str, Any], confidence: float, match_tier: str, needs_review: bool = False) -> MedDRARecord:
        return MedDRARecord(
            verbatim=verbatim,
            llt=entry.get("llt", entry["pt"]),
            llt_code=entry.get("llt_code", entry["pt_code"]),
            pt=entry["pt"],
            pt_code=entry["pt_code"],
            hlt=entry.get("hlt", "General disorders"),
            hlt_code=entry.get("hlt_code", "10000000"),
            hlgt=entry.get("hlgt", "General system disorders"),
            hlgt_code=entry.get("hlgt_code", "10000000"),
            soc=entry["soc"],
            soc_code=entry["soc_code"],
            confidence=confidence,
            match_tier=match_tier,
            needs_review=needs_review
        )


class WHODrugCoder:
    """Resolves verbatim concomitant medication terms to WHO Drug Global hierarchies and ATC codes."""

    def __init__(self, custom_dictionary: Optional[List[Dict[str, Any]]] = None):
        self.dictionary = custom_dictionary or WHO_DRUG_KNOWLEDGE_BASE
        self._build_index()

    def _build_index(self):
        self.exact_map: Dict[str, Dict[str, Any]] = {}
        self.synonym_map: Dict[str, Dict[str, Any]] = {}
        for entry in self.dictionary:
            p_name = entry["preferred_name"].lower().strip()
            self.exact_map[p_name] = entry
            for syn in entry.get("synonyms", []):
                s_key = normalize_clinical_term(syn)
                if s_key:
                    self.synonym_map[s_key] = entry

    def code_term(self, verbatim: str) -> WHODrugRecord:
        """Codes a single verbatim medication term through multi-tier matching."""
        if not verbatim or not verbatim.strip():
            return WHODrugRecord(
                verbatim="", preferred_name="", drug_record_number="",
                therapeutic_class="", atc_code="", atc_level1="", atc_level2="", atc_level3="", atc_level4="",
                confidence=0.0, match_tier="UNCODED", needs_review=True
            )

        raw_clean = verbatim.strip()
        norm = normalize_clinical_term(raw_clean)

        # Tier 1: Exact Match on Preferred Generic Name
        if raw_clean.lower() in self.exact_map:
            e = self.exact_map[raw_clean.lower()]
            return self._build_record(raw_clean, e, confidence=1.0, match_tier="EXACT_MATCH")

        # Tier 2: Normalized Synonym Match
        if norm in self.synonym_map:
            e = self.synonym_map[norm]
            return self._build_record(raw_clean, e, confidence=0.96, match_tier="SYNONYM_MATCH")

        # Tier 2b: Substring Match in Synonym Dictionary
        for syn_key, entry in self.synonym_map.items():
            if len(syn_key) > 3 and (syn_key in norm or norm in syn_key):
                return self._build_record(raw_clean, entry, confidence=0.90, match_tier="SYNONYM_SUBSTRING")

        # Tier 3: Fuzzy Levenshtein Match against Brand Names and Synonyms
        best_score = 0.0
        best_entry = None
        for syn_key, entry in self.synonym_map.items():
            score = levenshtein_similarity(norm, syn_key)
            if score > best_score:
                best_score = score
                best_entry = entry

        if best_score >= 0.75 and best_entry:
            return self._build_record(
                raw_clean, best_entry,
                confidence=round(best_score, 2),
                match_tier="FUZZY_MATCH",
                needs_review=(best_score < 0.85)
            )

        # Tier 4: Fallback Uncoded
        return WHODrugRecord(
            verbatim=raw_clean,
            preferred_name=raw_clean.upper(),
            drug_record_number="99999901",
            therapeutic_class="OTHER MEDICINAL PRODUCTS",
            atc_code="V03AX",
            atc_level1="V: VARIOUS",
            atc_level2="V03: ALL OTHER THERAPEUTIC PRODUCTS",
            atc_level3="V03A: ALL OTHER THERAPEUTIC PRODUCTS",
            atc_level4="V03AX: OTHER THERAPEUTIC PRODUCTS",
            confidence=0.40,
            match_tier="FALLBACK_UNCODED",
            needs_review=True
        )

    def _build_record(self, verbatim: str, entry: Dict[str, Any], confidence: float, match_tier: str, needs_review: bool = False) -> WHODrugRecord:
        return WHODrugRecord(
            verbatim=verbatim,
            preferred_name=entry["preferred_name"],
            drug_record_number=entry["drug_record_number"],
            therapeutic_class=entry["therapeutic_class"],
            atc_code=entry["atc_code"],
            atc_level1=entry["atc_level1"],
            atc_level2=entry["atc_level2"],
            atc_level3=entry["atc_level3"],
            atc_level4=entry["atc_level4"],
            confidence=confidence,
            match_tier=match_tier,
            needs_review=needs_review
        )


# ==================== SINGLETON CODER INSTANCES & CONVENIENCE METHODS ====================

MEDDRA_CODER = MedDRACoder()
WHO_DRUG_CODER = WHODrugCoder()


def code_meddra_term(verbatim: str) -> Dict[str, Any]:
    """Helper returning dictionary format for MedDRA coding."""
    return MEDDRA_CODER.code_term(verbatim).model_dump()


def code_whodrug_term(verbatim: str) -> Dict[str, Any]:
    """Helper returning dictionary format for WHO Drug coding."""
    return WHO_DRUG_CODER.code_term(verbatim).model_dump()


def auto_code_dataframe(df: pl.DataFrame, domain: str) -> Tuple[pl.DataFrame, Dict[str, Any]]:
    """
    Enriches a Polars DataFrame with standardized MedDRA or WHO Drug coding variables.
    Returns (enriched_df, coding_metrics_dict).
    """
    d = domain.upper()
    total_rows = len(df)
    if total_rows == 0:
        return df, {"total_terms": 0, "auto_coded": 0, "needs_review": 0, "accuracy_rate": "100%"}

    high_confidence_count = 0
    needs_review_count = 0

    if d in ("AE", "MH", "CE"):
        source_col = "AETERM" if "AETERM" in df.columns else ("MHTERM" if "MHTERM" in df.columns else None)
        if not source_col:
            # Look for any text column matching term
            for col in df.columns:
                if "TERM" in col:
                    source_col = col
                    break
        
        if not source_col:
            return df, {"error": "No verbatim term column (AETERM/MHTERM) found"}

        terms = df[source_col].to_list()
        coded_records = [MEDDRA_CODER.code_term(str(t) if t is not None else "") for t in terms]

        for r in coded_records:
            if r.confidence >= 0.85:
                high_confidence_count += 1
            else:
                needs_review_count += 1

        enriched = df.with_columns([
            pl.Series("AEDECOD", [r.pt for r in coded_records], dtype=pl.Utf8),
            pl.Series("AEPTCD", [r.pt_code for r in coded_records], dtype=pl.Utf8),
            pl.Series("AELLT", [r.llt for r in coded_records], dtype=pl.Utf8),
            pl.Series("AELLTCD", [r.llt_code for r in coded_records], dtype=pl.Utf8),
            pl.Series("AEHLT", [r.hlt for r in coded_records], dtype=pl.Utf8),
            pl.Series("AEHLGT", [r.hlgt for r in coded_records], dtype=pl.Utf8),
            pl.Series("AEBODSYS", [r.soc for r in coded_records], dtype=pl.Utf8),
            pl.Series("AESOCCD", [r.soc_code for r in coded_records], dtype=pl.Utf8),
            pl.Series("CODING_CONFIDENCE", [r.confidence for r in coded_records], dtype=pl.Float64),
            pl.Series("CODING_TIER", [r.match_tier for r in coded_records], dtype=pl.Utf8)
        ])

    elif d in ("CM", "SU", "PR"):
        source_col = "CMTRT" if "CMTRT" in df.columns else None
        if not source_col:
            for col in df.columns:
                if "TRT" in col or "DRUG" in col or "MED" in col:
                    source_col = col
                    break

        if not source_col:
            return df, {"error": "No verbatim treatment column (CMTRT) found"}

        terms = df[source_col].to_list()
        coded_records = [WHO_DRUG_CODER.code_term(str(t) if t is not None else "") for t in terms]

        for r in coded_records:
            if r.confidence >= 0.85:
                high_confidence_count += 1
            else:
                needs_review_count += 1

        enriched = df.with_columns([
            pl.Series("CMDECOD", [r.preferred_name for r in coded_records], dtype=pl.Utf8),
            pl.Series("CMCLAS", [r.therapeutic_class for r in coded_records], dtype=pl.Utf8),
            pl.Series("CMATC", [r.atc_code for r in coded_records], dtype=pl.Utf8),
            pl.Series("CMATC1", [r.atc_level1 for r in coded_records], dtype=pl.Utf8),
            pl.Series("CMATC2", [r.atc_level2 for r in coded_records], dtype=pl.Utf8),
            pl.Series("CMATC3", [r.atc_level3 for r in coded_records], dtype=pl.Utf8),
            pl.Series("CMATC4", [r.atc_level4 for r in coded_records], dtype=pl.Utf8),
            pl.Series("CODING_CONFIDENCE", [r.confidence for r in coded_records], dtype=pl.Float64),
            pl.Series("CODING_TIER", [r.match_tier for r in coded_records], dtype=pl.Utf8)
        ])

    else:
        return df, {"error": f"Domain {domain} does not require medical dictionary coding"}

    accuracy_rate = f"{(high_confidence_count / max(total_rows, 1)) * 100:.1f}%"
    return enriched, {
        "domain": domain,
        "total_terms": total_rows,
        "auto_coded_high_confidence": high_confidence_count,
        "needs_review": needs_review_count,
        "automation_rate": accuracy_rate
    }
