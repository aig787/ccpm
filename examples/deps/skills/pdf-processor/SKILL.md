---
name: pdf-processor
description: Process PDF files for text extraction, form filling, and document analysis. Use when you need to extract content from PDFs, fill forms, or analyze document structure.
---

# PDF Processor

## Instructions

When processing PDF files, follow these steps based on your specific needs:

### 1. Identify Processing Type
Determine what you need to do with the PDF:
- Extract text content
- Fill form fields
- Extract images or tables
- Merge or split PDFs
- Add annotations or watermarks
- Convert to other formats

### 2. Text Extraction

#### Basic Text Extraction
```python
import PyPDF2
import pdfplumber

# Method 1: Using PyPDF2
def extract_text_pypdf2(file_path):
    with open(file_path, 'rb') as file:
        reader = PyPDF2.PdfReader(file)
        text = ""
        for page in reader.pages:
            text += page.extract_text()
    return text

# Method 2: Using pdfplumber (better for tables)
def extract_text_pdfplumber(file_path):
    with pdfplumber.open(file_path) as pdf:
        text = ""
        for page in pdf.pages:
            text += page.extract_text() or ""
    return text
```

#### Advanced Text Extraction
- Preserve formatting and layout
- Handle multi-column documents
- Extract text from specific regions
- Process scanned PDFs with OCR

### 3. Form Processing

#### Form Field Detection
```python
def detect_form_fields(file_path):
    reader = PyPDF2.PdfReader(file_path)
    fields = {}
    if reader.get_fields():
        for field_name, field in reader.get_fields().items():
            fields[field_name] = {
                'type': field.field_type,
                'value': field.value,
                'required': field.required if hasattr(field, 'required') else False
            }
    return fields

def fill_form_fields(file_path, output_path, field_data):
    reader = PyPDF2.PdfReader(file_path)
    writer = PyPDF2.PdfWriter()

    for page in reader.pages:
        writer.add_page(page)

    if writer.get_fields():
        for field_name, value in field_data.items():
            if field_name in writer.get_fields():
                writer.get_fields()[field_name].value = value

    with open(output_path, 'wb') as output_file:
        writer.write(output_file)
```

#### Common Form Types
- Application forms
- Invoices and receipts
- Survey forms
- Legal documents
- Medical forms

### 4. Content Analysis

#### Structure Analysis
```python
def analyze_pdf_structure(file_path):
    with pdfplumber.open(file_path) as pdf:
        analysis = {
            'pages': len(pdf.pages),
            'has_images': False,
            'has_tables': False,
            'has_forms': False,
            'text_density': [],
            'sections': []
        }

        for i, page in enumerate(pdf.pages):
            # Check for images
            if page.images:
                analysis['has_images'] = True

            # Check for tables
            if page.extract_tables():
                analysis['has_tables'] = True

            # Calculate text density
            text = page.extract_text()
            if text:
                density = len(text) / (page.width * page.height)
                analysis['text_density'].append(density)

            # Detect section headers (basic heuristic)
            lines = text.split('\n') if text else []
            for line in lines:
                if line.isupper() and len(line) < 50:
                    analysis['sections'].append({
                        'page': i + 1,
                        'title': line.strip()
                    })

    return analysis
```

#### Table Extraction
```python
def extract_tables(file_path):
    tables = []
    with pdfplumber.open(file_path) as pdf:
        for page_num, page in enumerate(pdf.pages):
            page_tables = page.extract_tables()
            for table in page_tables:
                tables.append({
                    'page': page_num + 1,
                    'data': table,
                    'rows': len(table),
                    'columns': len(table[0]) if table else 0
                })
    return tables
```

### 5. PDF Manipulation

#### Merge PDFs
```python
from PyPDF2 import PdfMerger

def merge_pdfs(file_paths, output_path):
    merger = PdfMerger()
    for path in file_paths:
        merger.append(path)
    merger.write(output_path)
    merger.close()
```

#### Split PDF
```python
def split_pdf(file_path, output_dir):
    reader = PyPDF2.PdfReader(file_path)
    for i, page in enumerate(reader.pages):
        writer = PyPDF2.PdfWriter()
        writer.add_page(page)
        output_path = f"{output_dir}/page_{i+1}.pdf"
        with open(output_path, 'wb') as output_file:
            writer.write(output_file)
```

#### Add Watermark
```python
def add_watermark(input_path, output_path, watermark_text):
    reader = PyPDF2.PdfReader(input_path)
    writer = PyPDF2.PdfWriter()

    for page in reader.pages:
        writer.add_page(page)
        # Add watermark logic here
        # This requires additional libraries like reportlab

    with open(output_path, 'wb') as output_file:
        writer.write(output_file)
```

### 6. OCR for Scanned PDFs

#### Using Tesseract OCR
```python
import pytesseract
from PIL import Image
import fitz  # PyMuPDF

def ocr_pdf(file_path):
    doc = fitz.open(file_path)
    text = ""

    for page_num in range(len(doc)):
        page = doc.load_page(page_num)
        pix = page.get_pixmap()
        img = Image.frombytes("RGB", [pix.width, pix.height], pix.samples)
        text += pytesseract.image_to_string(img)

    return text
```

### 7. Error Handling

#### Common Issues
- Password-protected PDFs
- Corrupted files
- Unsupported formats
- Memory issues with large files
- Encoding problems

#### Error Handling Pattern
```python
import logging

def process_pdf_safely(file_path, processing_func):
    try:
        # Check if file exists
        if not os.path.exists(file_path):
            raise FileNotFoundError(f"File not found: {file_path}")

        # Check file size
        file_size = os.path.getsize(file_path)
        if file_size > 100 * 1024 * 1024:  # 100MB limit
            logging.warning(f"Large file detected: {file_size} bytes")

        # Process the file
        result = processing_func(file_path)
        return result

    except Exception as e:
        logging.error(f"Error processing PDF {file_path}: {str(e)}")
        raise
```

### 8. Performance Optimization

#### For Large Files
- Process pages in chunks
- Use generators for memory efficiency
- Implement progress tracking
- Consider parallel processing

#### Batch Processing
```python
import concurrent.futures
import os

def batch_process_pdfs(directory, processing_func, max_workers=4):
    pdf_files = [f for f in os.listdir(directory) if f.endswith('.pdf')]

    with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as executor:
        futures = []
        for pdf_file in pdf_files:
            file_path = os.path.join(directory, pdf_file)
            future = executor.submit(processing_func, file_path)
            futures.append((pdf_file, future))

        results = {}
        for pdf_file, future in futures:
            try:
                results[pdf_file] = future.result()
            except Exception as e:
                results[pdf_file] = f"Error: {str(e)}"

    return results
```

## Usage Examples

### Example 1: Extract Text from Invoice
1. Load the PDF invoice
2. Extract all text content
3. Parse for invoice number, date, amount
4. Save extracted data to structured format

### Example 2: Fill Application Form
1. Load the application form PDF
2. Detect all form fields
3. Fill fields with provided data
4. Save filled form as new PDF

### Example 3: Extract Tables from Report
1. Open multi-page report PDF
2. Extract all tables from each page
3. Convert tables to CSV or Excel
4. Preserve table structure and formatting

## Required Libraries

Install necessary Python packages:
```bash
pip install PyPDF2 pdfplumber PyMuPDF pytesseract pillow
```

## Tips

- Always check if PDF is password-protected first
- Use different libraries based on your needs (speed vs accuracy)
- For scanned documents, OCR quality depends on image resolution
- Consider the PDF version when working with older files
- Test with sample pages before processing entire documents
- Handle encoding issues for non-English text