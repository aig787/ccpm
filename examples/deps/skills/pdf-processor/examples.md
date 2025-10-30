# PDF Processor Examples

## Example 1: Basic Text Extraction

### Simple Text Extraction
```bash
# Extract text to file
python scripts/pdf_extractor.py document.pdf --output extracted_text.txt

# Extract text to JSON with metadata
python scripts/pdf_extractor.py document.pdf --format json --output document_data.json
```

### Output Sample:
```json
{
  "text": "--- Page 1 ---\nAnnual Report 2024\nCompany Name Inc.\n\n--- Page 2 ---\nFinancial Highlights...",
  "metadata": {
    "title": "Annual Report 2024",
    "author": "Company Name",
    "page_count": 10,
    "file_size": 2048576
  },
  "tables": [...],
  "forms": {...}
}
```

## Example 2: Extract Tables and Export to Excel

### Command:
```bash
# Extract tables and save to Excel
python scripts/pdf_extractor.py financial_report.pdf \
  --extract-tables \
  --export-excel \
  --excel-path financial_tables.xlsx
```

### Generated Excel Structure:
- `Table_1_P1` - First table from page 1
- `Table_2_P3` - First table from page 3
- `Table_3_P5` - Table from page 5

## Example 3: OCR on Scanned PDFs

### OCR Processing:
```bash
# Perform OCR and save text
python scripts/pdf_extractor.py scanned_document.pdf \
  --ocr \
  --ocr-dir ocr_images \
  --output ocr_text.txt

# Combined OCR and table extraction
python scripts/pdf_extractor.py scanned_report.pdf \
  --ocr \
  --extract-tables \
  --use-pdfplumber
```

### OCR Output Directory Structure:
```
ocr_images/
├── page_1.png
├── page_2.png
├── page_3.png
└── ...
```

## Example 4: Form Field Detection and Filling

### Detect Form Fields:
```bash
python scripts/pdf_extractor.py application_form.pdf --format json --output form_analysis.json
```

### Output Form Fields:
```json
{
  "forms": {
    "first_name": {
      "type": "/Tx",
      "value": "",
      "required": true
    },
    "last_name": {
      "type": "/Tx",
      "value": "",
      "required": true
    },
    "email": {
      "type": "/Tx",
      "value": "",
      "required": true
    },
    "signature": {
      "type": "/Sig",
      "value": "",
      "required": true
    }
  }
}
```

### Fill Form Fields:
```json
// form_data.json
{
  "first_name": "John",
  "last_name": "Doe",
  "email": "john.doe@example.com",
  "phone": "555-0123",
  "address": "123 Main St",
  "city": "Anytown",
  "state": "CA",
  "zip_code": "12345"
}
```

```bash
python scripts/pdf_extractor.py application_form.pdf \
  --fill-form form_data.json \
  --form-output filled_application.pdf
```

## Example 5: PDF Manipulation

### Split PDF into Pages:
```bash
# Split each page into separate files
python scripts/pdf_extractor.py large_document.pdf --split output_pages/

# Split specific page ranges
python scripts/pdf_extractor.py report.pdf --split sections/
```

### Output:
```
sections/
├── page_1.pdf
├── page_2.pdf
├── page_3.pdf
└── ...
```

### Merge Multiple PDFs:
```bash
python scripts/pdf_extractor.py main_document.pdf \
  --merge appendix1.pdf appendix2.pdf appendix3.pdf \
  --merge-output complete_document.pdf
```

## Example 6: Batch Processing Multiple PDFs

### Python Batch Script:
```python
#!/usr/bin/env python3
import os
import json
from pathlib import Path
from pdf_extractor import PDFProcessor

def process_directory(input_dir, output_dir):
    """Process all PDFs in a directory"""
    results = []

    for pdf_file in Path(input_dir).glob("*.pdf"):
        print(f"Processing {pdf_file.name}...")

        processor = PDFProcessor(pdf_file)

        options = {
            "output_format": "json",
            "extract_tables": True,
            "detect_forms": True,
            "use_pdfplumber": True
        }

        result = processor.process(options)

        # Save individual result
        output_file = Path(output_dir) / f"{pdf_file.stem}_processed.json"
        with open(output_file, 'w') as f:
            json.dump(result, f, indent=2, default=str)

        results.append({
            "file": pdf_file.name,
            "pages": result.get("metadata", {}).get("page_count", 0),
            "tables": len(result.get("tables", [])),
            "forms": len(result.get("forms", {})),
            "text_length": len(result.get("text", ""))
        })

    # Save summary
    summary_file = Path(output_dir) / "batch_summary.json"
    with open(summary_file, 'w') as f:
        json.dump(results, f, indent=2)

    return results

# Usage
if __name__ == "__main__":
    results = process_directory("input_pdfs/", "output_results/")
    print(f"Processed {len(results)} PDF files")
```

## Example 7: Invoice Processing Workflow

### Complete Invoice Processing:
```python
#!/usr/bin/env python3
import json
import re
from datetime import datetime
from pdf_extractor import PDFProcessor

def process_invoice(pdf_path):
    """Extract and analyze invoice data"""
    processor = PDFProcessor(pdf_path)

    # Extract content
    options = {
        "extract_tables": True,
        "use_pdfplumber": True,
        "detect_forms": True
    }

    content = processor.process(options)

    # Parse invoice information
    invoice_data = {
        "metadata": content.get("metadata", {}),
        "extracted_at": datetime.now().isoformat(),
        "total_amount": extract_total_amount(content["text"]),
        "invoice_number": extract_invoice_number(content["text"]),
        "vendor": extract_vendor(content["text"]),
        "line_items": extract_line_items(content.get("tables", []))
    }

    return invoice_data

def extract_total_amount(text):
    """Extract total amount from text"""
    patterns = [
        r"Total[:\s]*\$?([\d,]+\.\d{2})",
        r"Amount Due[:\s]*\$?([\d,]+\.\d{2})",
        r"Grand Total[:\s]*\$?([\d,]+\.\d{2})"
    ]

    for pattern in patterns:
        match = re.search(pattern, text, re.IGNORECASE)
        if match:
            return float(match.group(1).replace(",", ""))
    return None

def extract_invoice_number(text):
    """Extract invoice number"""
    patterns = [
        r"Invoice[:\s#]*([A-Z0-9-]+)",
        r"Inv[:\s#]*([A-Z0-9-]+)",
        r"Bill[:\s#]*([A-Z0-9-]+)"
    ]

    for pattern in patterns:
        match = re.search(pattern, text, re.IGNORECASE)
        if match:
            return match.group(1)
    return None

def extract_vendor(text):
    """Extract vendor name from top of document"""
    lines = text.split('\n')[:10]  # Check first 10 lines
    for line in lines:
        if len(line) > 5 and not any(skip in line.lower() for skip in ['invoice', 'bill', 'date', 'page']):
            return line.strip()
    return None

def extract_line_items(tables):
    """Extract line items from tables"""
    items = []

    for table in tables:
        if not table["data"]:
            continue

        # Look for table with item columns
        headers = [col.lower() if col else "" for col in table["data"][0]]

        if any(keyword in ' '.join(headers) for keyword in ['description', 'item', 'product']):
            for row in table["data"][1:]:
                if len(row) >= 2 and row[0]:  # Skip empty rows
                    items.append({
                        "description": row[0],
                        "quantity": row[1] if len(row) > 1 else "",
                        "price": row[2] if len(row) > 2 else "",
                        "total": row[3] if len(row) > 3 else ""
                    })

    return items

# Usage
invoice_data = process_invoice("invoice.pdf")
with open("invoice_data.json", "w") as f:
    json.dump(invoice_data, f, indent=2, default=str)
```

## Example 8: Form Template Automation

### Automated Form Filling:
```python
#!/usr/bin/env python3
import json
from datetime import datetime
from pdf_extractor import PDFProcessor

def fill_job_application(template_pdf, applicant_data, output_path):
    """Fill job application form with applicant data"""

    # Load form field template
    with open("templates/form-data-template.json") as f:
        templates = json.load(f)

    # Map applicant data to form fields
    form_data = {}
    job_template = templates["form_templates"]["job_application"]["fields"]

    for field in job_template:
        if field in applicant_data:
            form_data[field] = applicant_data[field]
        elif field == "signature_date":
            form_data[field] = datetime.now().strftime("%m/%d/%Y")

    # Fill the form
    processor = PDFProcessor(template_pdf)
    success = processor.fill_form_fields(form_data, output_path)

    return success

# Example applicant data
applicant = {
    "first_name": "Jane",
    "last_name": "Smith",
    "email": "jane.smith@email.com",
    "phone": "(555) 123-4567",
    "address": "456 Oak Ave",
    "city": "Springfield",
    "state": "IL",
    "zip_code": "62701",
    "position": "Software Engineer",
    "salary_expectation": "$85,000",
    "start_date": "03/01/2024"
}

# Fill the form
success = fill_job_application(
    "job_application_template.pdf",
    applicant,
    "filled_application.pdf"
)

if success:
    print("Application form filled successfully!")
else:
    print("Failed to fill application form")
```

## Example 9: Research Paper Analysis

### Extract and Analyze Research Papers:
```python
#!/usr/bin/env python3
import re
import json
from pdf_extractor import PDFProcessor

def analyze_research_paper(pdf_path):
    """Extract and analyze academic paper content"""
    processor = PDFProcessor(pdf_path)

    options = {
        "extract_tables": True,
        "use_pdfplumber": True
    }

    content = processor.process(options)
    text = content["text"]

    analysis = {
        "metadata": content.get("metadata", {}),
        "abstract": extract_abstract(text),
        "keywords": extract_keywords(text),
        "sections": extract_sections(text),
        "references": count_references(text),
        "tables": len(content.get("tables", [])),
        "figures": count_figures(text),
        "citations": extract_citations(text)
    }

    return analysis

def extract_abstract(text):
    """Extract abstract section"""
    match = re.search(r'ABSTRACT[:\s]*(.*?)(?=\n\s*[A-Z]|\nKeywords)', text, re.DOTALL | re.IGNORECASE)
    return match.group(1).strip() if match else None

def extract_keywords(text):
    """Extract keywords"""
    match = re.search(r'Keywords?[:\s]*(.*?)(?=\n|\r)', text, re.IGNORECASE)
    if match:
        return [k.strip() for k in match.group(1).split(',')]
    return []

def extract_sections(text):
    """Extract paper sections"""
    section_pattern = r'\n\s*([A-Z][A-Z\s]+)\s*\n'
    sections = re.findall(section_pattern, text)
    return [s.strip() for s in sections if len(s.strip()) > 3]

def count_references(text):
    """Count references in bibliography"""
    ref_match = re.search(r'REFERENCES[:\s]*(.*)', text, re.DOTALL | re.IGNORECASE)
    if ref_match:
        refs = re.findall(r'\n\s*\[\d+\]', ref_match.group(1))
        return len(refs)
    return 0

def count_figures(text):
    """Count figure references"""
    figure_refs = re.findall(r'Figure\s+\d+', text, re.IGNORECASE)
    return len(figure_refs)

def extract_citations(text):
    """Extract in-text citations"""
    citations = re.findall(r'\[(\d+(?:,\s*\d+)*)\]', text)
    return citations[:20]  # Return first 20 citations

# Usage
analysis = analyze_research_paper("research_paper.pdf")
with open("paper_analysis.json", "w") as f:
    json.dump(analysis, f, indent=2, default=str)

print(f"Paper Analysis:")
print(f"- Sections: {len(analysis['sections'])}")
print(f"- Keywords: {', '.join(analysis['keywords'])}")
print(f"- References: {analysis['references']}")
print(f"- Figures: {analysis['figures']}")
```

## Example 10: Legal Document Processing

### Contract Analysis and Extraction:
```python
#!/usr/bin/env python3
import re
from datetime import datetime
from pdf_extractor import PDFProcessor

def process_contract(pdf_path):
    """Extract key information from legal contracts"""
    processor = PDFProcessor(pdf_path)

    options = {
        "detect_forms": True,
        "use_pdfplumber": True
    }

    content = processor.process(options)
    text = content["text"]

    contract_info = {
        "parties": extract_parties(text),
        "effective_date": extract_date(text, "effective"),
        "termination_date": extract_date(text, "termination"),
        "signatures": extract_signatures(text),
        "key_terms": extract_key_terms(text),
        "obligations": extract_obligations(text),
        "forms_detected": content.get("forms", {})
    }

    return contract_info

def extract_parties(text):
    """Extract contract parties"""
    party_patterns = [
        r'between\s+([^,\n]+)\s+and\s+([^,\n]+)',
        r'PARTIES?:?\s*(.*?)(?=\nWHEREAS|\nNOW)',
        r'([A-Z][a-z]+\s+[A-Z][a-z]+(?:\s+(?:Inc|LLC|Corp|Ltd))?)'
    ]

    parties = []
    for pattern in party_patterns:
        matches = re.findall(pattern, text, re.IGNORECASE)
        parties.extend(matches)

    return list(set(parties))

def extract_date(text, date_type):
    """Extract specific dates from contract"""
    patterns = {
        "effective": [
            r'effective\s+date[:\s]*(\d{1,2}[/-]\d{1,2}[/-]\d{4})',
            r'commences?\s+on[:\s]*(\d{1,2}[/-]\d{1,2}[/-]\d{4})'
        ],
        "termination": [
            r'terminat(?:e|ion)[:\s]*(\d{1,2}[/-]\d{1,2}[/-]\d{4})',
            r'expire[s]?:?\s*(\d{1,2}[/-]\d{1,2}[/-]\d{4})'
        ]
    }

    if date_type in patterns:
        for pattern in patterns[date_type]:
            match = re.search(pattern, text, re.IGNORECASE)
            if match:
                return match.group(1)

    return None

def extract_signatures(text):
    """Extract signature blocks"""
    sig_pattern = r'(?:Signature|Signed)[:\s]*\n\s*([^\n]+)\s*\n.*?(\d{1,2}[/-]\d{1,2}[/-]\d{4})'
    signatures = re.findall(sig_pattern, text, re.IGNORECASE)

    return [{"name": sig[0].strip(), "date": sig[1]} for sig in signatures]

def extract_key_terms(text):
    """Extract key contractual terms"""
    terms = []
    term_patterns = [
        r'term[s]?[:\s]*(.*?)(?=\n|$)',
        r'duration[:\s]*(.*?)(?=\n|$)',
        r'period[:\s]*(.*?)(?=\n|$)'
    ]

    for pattern in term_patterns:
        matches = re.findall(pattern, text, re.IGNORECASE)
        terms.extend(matches)

    return [t.strip() for t in terms if t.strip()]

def extract_obligations(text):
    """Extract obligations and responsibilities"""
    obligations = []

    # Look for sections with "shall", "must", "will"
    obligation_patterns = [
        r'shall\s+([^.!?]*[.!?])',
        r'must\s+([^.!?]*[.!?])',
        r'will\s+([^.!?]*[.!?])'
    ]

    for pattern in obligation_patterns:
        matches = re.findall(pattern, text, re.IGNORECASE)
        obligations.extend(matches)

    return [o.strip() for o in obligations[:20]]  # Return first 20

# Usage
contract_data = process_contract("service_agreement.pdf")
print("Contract Analysis:")
print(f"- Parties: {contract_data['parties']}")
print(f"- Effective Date: {contract_data['effective_date']}")
print(f"- Signatures: {len(contract_data['signatures'])}")
print(f"- Key Obligations: {len(contract_data['obligations'])}")
```

## Installation Requirements

Install required Python packages:

```bash
# Core functionality
pip install PyPDF2 pdfplumber

# OCR support
pip install pytesseract pillow
# Also install Tesseract OCR system:
# macOS: brew install tesseract
# Ubuntu: sudo apt-get install tesseract-ocr
# Windows: Download from https://github.com/UB-Mannheim/tesseract/wiki

# Advanced features
pip install PyMuPDF pandas openpyxl

# All dependencies
pip install PyPDF2 pdfplumber PyMuPDF pytesseract pillow pandas openpyxl
```

## Error Handling

### Common Issues and Solutions:

1. **Encrypted PDFs**: Password-protected PDFs require password
2. **Scanned PDFs**: Use OCR option for image-based content
3. **Large Files**: Process in chunks for memory efficiency
4. **Corrupted Files**: Try different PDF libraries
5. **Missing Libraries**: Install required dependencies

### Example Error Handling:
```python
try:
    processor = PDFProcessor("document.pdf")
    result = processor.process(options)
except Exception as e:
    print(f"Error processing PDF: {e}")
    # Try alternative method
    options["use_pdfplumber"] = False
    result = processor.process(options)
```