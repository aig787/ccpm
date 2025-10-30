#!/usr/bin/env python3
"""
PDF Processor Tool
Comprehensive PDF processing including text extraction, form filling, and OCR
"""

import os
import sys
import json
import argparse
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Any
import logging

# Set up logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

try:
    import PyPDF2
    HAS_PYPDF2 = True
except ImportError:
    HAS_PYPDF2 = False
    logger.warning("PyPDF2 not installed - some features will be unavailable")

try:
    import pdfplumber
    HAS_PDFPLUMBER = True
except ImportError:
    HAS_PDFPLUMBER = False
    logger.warning("pdfplumber not installed - table extraction will be unavailable")

try:
    import fitz  # PyMuPDF
    HAS_PYMUPDF = True
except ImportError:
    HAS_PYMUPDF = False
    logger.warning("PyMuPDF not installed - OCR and advanced features will be unavailable")

try:
    import pytesseract
    from PIL import Image
    HAS_TESSERACT = True
except ImportError:
    HAS_TESSERACT = False
    logger.warning("Tesseract OCR not installed - OCR features will be unavailable")

try:
    import pandas as pd
    HAS_PANDAS = True
except ImportError:
    HAS_PANDAS = False
    logger.warning("pandas not installed - Excel export will be unavailable")

class PDFProcessor:
    def __init__(self, file_path: str):
        self.file_path = Path(file_path)
        self.content = {
            "text": "",
            "metadata": {},
            "tables": [],
            "images": [],
            "forms": {},
            "pages": []
        }

    def validate_file(self) -> bool:
        """Validate PDF file"""
        if not self.file_path.exists():
            logger.error(f"File not found: {self.file_path}")
            return False

        if self.file_path.suffix.lower() != '.pdf':
            logger.error(f"File is not a PDF: {self.file_path}")
            return False

        # Check file size
        file_size_mb = self.file_path.stat().st_size / (1024 * 1024)
        if file_size_mb > 100:
            logger.warning(f"Large file detected: {file_size_mb:.2f} MB")

        return True

    def extract_metadata(self) -> Dict:
        """Extract PDF metadata"""
        metadata = {}

        if HAS_PYMUPDF:
            try:
                doc = fitz.open(self.file_path)
                metadata = doc.metadata
                metadata.update({
                    "page_count": doc.page_count,
                    "is_pdf": True,
                    "is_encrypted": doc.needs_pass,
                    "file_size": self.file_path.stat().st_size
                })
                doc.close()
            except Exception as e:
                logger.error(f"Error extracting metadata with PyMuPDF: {e}")

        elif HAS_PYPDF2:
            try:
                with open(self.file_path, 'rb') as file:
                    reader = PyPDF2.PdfReader(file)
                    metadata = reader.metadata or {}
                    metadata.update({
                        "page_count": len(reader.pages),
                        "is_encrypted": reader.is_encrypted,
                        "file_size": self.file_path.stat().st_size
                    })
            except Exception as e:
                logger.error(f"Error extracting metadata with PyPDF2: {e}")

        self.content["metadata"] = metadata
        return metadata

    def extract_text_pypdf2(self) -> str:
        """Extract text using PyPDF2"""
        if not HAS_PYPDF2:
            return ""

        text = ""
        try:
            with open(self.file_path, 'rb') as file:
                reader = PyPDF2.PdfReader(file)
                for page_num, page in enumerate(reader.pages):
                    page_text = page.extract_text()
                    if page_text:
                        text += f"\n--- Page {page_num + 1} ---\n"
                        text += page_text + "\n"
        except Exception as e:
            logger.error(f"Error extracting text with PyPDF2: {e}")

        return text

    def extract_text_pdfplumber(self) -> str:
        """Extract text using pdfplumber"""
        if not HAS_PDFPLUMBER:
            return ""

        text = ""
        try:
            with pdfplumber.open(self.file_path) as pdf:
                for page_num, page in enumerate(pdf.pages):
                    page_text = page.extract_text()
                    if page_text:
                        text += f"\n--- Page {page_num + 1} ---\n"
                        text += page_text + "\n"
        except Exception as e:
            logger.error(f"Error extracting text with pdfplumber: {e}")

        return text

    def extract_tables(self) -> List[Dict]:
        """Extract tables from PDF"""
        if not HAS_PDFPLUMBER:
            logger.warning("pdfplumber not available - cannot extract tables")
            return []

        tables = []
        try:
            with pdfplumber.open(self.file_path) as pdf:
                for page_num, page in enumerate(pdf.pages):
                    page_tables = page.extract_tables()
                    for table_num, table in enumerate(page_tables):
                        if table and len(table) > 1:  # At least header and one row
                            tables.append({
                                "page": page_num + 1,
                                "table_number": table_num + 1,
                                "rows": len(table),
                                "columns": len(table[0]) if table else 0,
                                "data": table
                            })
        except Exception as e:
            logger.error(f"Error extracting tables: {e}")

        self.content["tables"] = tables
        return tables

    def extract_images(self) -> List[Dict]:
        """Extract images from PDF"""
        if not HAS_PYMUPDF:
            logger.warning("PyMuPDF not available - cannot extract images")
            return []

        images = []
        try:
            doc = fitz.open(self.file_path)
            for page_num in range(doc.page_count):
                page = doc.load_page(page_num)
                image_list = page.get_images()

                for img_index, img in enumerate(image_list):
                    xref = img[0]
                    base_image = doc.extract_image(xref)
                    image_bytes = base_image["image"]
                    image_ext = base_image["ext"]

                    images.append({
                        "page": page_num + 1,
                        "index": img_index,
                        "xref": xref,
                        "extension": image_ext,
                        "size": len(image_bytes),
                        "width": base_image.get("width"),
                        "height": base_image.get("height")
                    })

            doc.close()
        except Exception as e:
            logger.error(f"Error extracting images: {e}")

        self.content["images"] = images
        return images

    def detect_form_fields(self) -> Dict:
        """Detect form fields in PDF"""
        if not HAS_PYPDF2:
            logger.warning("PyPDF2 not available - cannot detect form fields")
            return {}

        form_fields = {}
        try:
            with open(self.file_path, 'rb') as file:
                reader = PyPDF2.PdfReader(file)
                if reader.get_fields():
                    for field_name, field in reader.get_fields().items():
                        form_fields[field_name] = {
                            "type": field.field_type,
                            "value": field.value,
                            "required": getattr(field, 'required', False),
                            "flags": getattr(field, 'flags', 0)
                        }
        except Exception as e:
            logger.error(f"Error detecting form fields: {e}")

        self.content["forms"] = form_fields
        return form_fields

    def perform_ocr(self, output_dir: Optional[str] = None) -> str:
        """Perform OCR on scanned PDF"""
        if not HAS_TESSERACT or not HAS_PYMUPDF:
            logger.error("Tesseract OCR and PyMuPDF required for OCR")
            return ""

        text = ""
        try:
            doc = fitz.open(self.file_path)

            if output_dir:
                output_dir = Path(output_dir)
                output_dir.mkdir(exist_ok=True)

            for page_num in range(doc.page_count):
                page = doc.load_page(page_num)
                pix = page.get_pixmap()
                img_data = pix.tobytes("png")

                # Save image if output directory specified
                if output_dir:
                    img_path = output_dir / f"page_{page_num + 1}.png"
                    with open(img_path, 'wb') as img_file:
                        img_file.write(img_data)

                # Perform OCR
                img = Image.open(io.BytesIO(img_data))
                page_text = pytesseract.image_to_string(img)
                text += f"\n--- OCR Page {page_num + 1} ---\n"
                text += page_text + "\n"

            doc.close()
        except Exception as e:
            logger.error(f"Error performing OCR: {e}")

        return text

    def fill_form_fields(self, field_data: Dict, output_path: str) -> bool:
        """Fill form fields in PDF"""
        if not HAS_PYPDF2:
            logger.error("PyPDF2 required for form filling")
            return False

        try:
            reader = PyPDF2.PdfReader(self.file_path)
            writer = PyPDF2.PdfWriter()

            # Copy all pages
            for page in reader.pages:
                writer.add_page(page)

            # Fill form fields
            if writer.get_fields():
                for field_name, value in field_data.items():
                    if field_name in writer.get_fields():
                        writer.get_fields()[field_name].value = str(value)

            # Save filled form
            with open(output_path, 'wb') as output_file:
                writer.write(output_file)

            logger.info(f"Filled form saved to: {output_path}")
            return True
        except Exception as e:
            logger.error(f"Error filling form fields: {e}")
            return False

    def split_pdf(self, output_dir: str, split_ranges: Optional[List[Tuple[int, int]]] = None) -> List[str]:
        """Split PDF into multiple files"""
        if not HAS_PYPDF2:
            logger.error("PyPDF2 required for PDF splitting")
            return []

        output_files = []
        try:
            reader = PyPDF2.PdfReader(self.file_path)

            if split_ranges:
                # Split by specified ranges
                for i, (start, end) in enumerate(split_ranges):
                    writer = PyPDF2.PdfWriter()
                    for page_num in range(start - 1, min(end, len(reader.pages))):
                        writer.add_page(reader.pages[page_num])

                    output_path = Path(output_dir) / f"split_{i + 1}.pdf"
                    with open(output_path, 'wb') as output_file:
                        writer.write(output_file)
                    output_files.append(str(output_path))
            else:
                # Split each page
                for page_num, page in enumerate(reader.pages):
                    writer = PyPDF2.PdfWriter()
                    writer.add_page(page)

                    output_path = Path(output_dir) / f"page_{page_num + 1}.pdf"
                    with open(output_path, 'wb') as output_file:
                        writer.write(output_file)
                    output_files.append(str(output_path))

            logger.info(f"Split into {len(output_files)} files")
        except Exception as e:
            logger.error(f"Error splitting PDF: {e}")

        return output_files

    def merge_pdfs(self, pdf_files: List[str], output_path: str) -> bool:
        """Merge multiple PDFs"""
        if not HAS_PYPDF2:
            logger.error("PyPDF2 required for PDF merging")
            return False

        try:
            merger = PyPDF2.PdfMerger()

            # Add current file first
            merger.append(str(self.file_path))

            # Add additional files
            for pdf_file in pdf_files:
                merger.append(pdf_file)

            # Write merged PDF
            with open(output_path, 'wb') as output_file:
                merger.write(output_path)

            merger.close()
            logger.info(f"Merged PDF saved to: {output_path}")
            return True
        except Exception as e:
            logger.error(f"Error merging PDFs: {e}")
            return False

    def export_tables_to_excel(self, output_path: str) -> bool:
        """Export extracted tables to Excel"""
        if not HAS_PANDAS:
            logger.error("pandas required for Excel export")
            return False

        if not self.content["tables"]:
            logger.warning("No tables to export")
            return False

        try:
            with pd.ExcelWriter(output_path, engine='openpyxl') as writer:
                for i, table in enumerate(self.content["tables"]):
                    df = pd.DataFrame(table["data"][1:], columns=table["data"][0])
                    sheet_name = f"Table_{i + 1}_P{table['page']}"
                    df.to_excel(writer, sheet_name=sheet_name, index=False)

            logger.info(f"Tables exported to: {output_path}")
            return True
        except Exception as e:
            logger.error(f"Error exporting tables: {e}")
            return False

    def process(self, options: Dict) -> Dict:
        """Main processing function"""
        if not self.validate_file():
            return {"error": "Invalid PDF file"}

        logger.info(f"Processing PDF: {self.file_path}")

        # Extract metadata
        self.extract_metadata()

        # Extract text
        text = ""
        if options.get("use_pdfplumber", HAS_PDFPLUMBER):
            text = self.extract_text_pdfplumber()
        elif options.get("use_pypdf2", HAS_PYPDF2):
            text = self.extract_text_pypdf2()

        # Perform OCR if requested
        if options.get("ocr", False) and HAS_TESSERACT and HAS_PYMUPDF:
            ocr_text = self.perform_ocr(options.get("ocr_output_dir"))
            text += "\n\n--- OCR Content ---\n" + ocr_text

        self.content["text"] = text

        # Extract additional content
        if options.get("extract_tables", True):
            self.extract_tables()

        if options.get("extract_images", False):
            self.extract_images()

        if options.get("detect_forms", True):
            self.detect_form_fields()

        # Export results
        if options.get("output_format") == "json":
            output_path = options.get("output_path", self.file_path.with_suffix('.json'))
            with open(output_path, 'w') as f:
                json.dump(self.content, f, indent=2, default=str)
        elif options.get("output_format") == "txt":
            output_path = options.get("output_path", self.file_path.with_suffix('.txt'))
            with open(output_path, 'w') as f:
                f.write(text)

        # Export tables to Excel if requested
        if options.get("export_excel", False) and self.content["tables"]:
            excel_path = options.get("excel_path", self.file_path.with_suffix('.xlsx'))
            self.export_tables_to_excel(excel_path)

        logger.info("Processing complete")
        return self.content

def main():
    parser = argparse.ArgumentParser(description="Process PDF files")
    parser.add_argument("file", help="PDF file to process")
    parser.add_argument("--output", "-o", help="Output file path")
    parser.add_argument("--format", choices=["txt", "json"], default="txt", help="Output format")
    parser.add_argument("--ocr", action="store_true", help="Perform OCR on scanned PDFs")
    parser.add_argument("--ocr-dir", help="Directory to save OCR images")
    parser.add_argument("--extract-tables", action="store_true", default=True, help="Extract tables")
    parser.add_argument("--extract-images", action="store_true", help="Extract images")
    parser.add_argument("--detect-forms", action="store_true", default=True, help="Detect form fields")
    parser.add_argument("--export-excel", action="store_true", help="Export tables to Excel")
    parser.add_argument("--excel-path", help="Excel output path")
    parser.add_argument("--use-pdfplumber", action="store_true", help="Use pdfplumber for text extraction")
    parser.add_argument("--fill-form", help="JSON file with form field data")
    parser.add_argument("--form-output", help="Output path for filled form")
    parser.add_argument("--split", help="Directory to split PDF into pages")
    parser.add_argument("--merge", nargs="+", help="Additional PDF files to merge")
    parser.add_argument("--merge-output", help="Output path for merged PDF")

    args = parser.parse_args()

    # Initialize processor
    processor = PDFProcessor(args.file)

    # Handle special operations
    if args.fill_form:
        with open(args.fill_form, 'r') as f:
            form_data = json.load(f)
        success = processor.fill_form_fields(form_data, args.form_output)
        sys.exit(0 if success else 1)

    if args.split:
        output_files = processor.split_pdf(args.split)
        print(f"Split into {len(output_files)} files")
        sys.exit(0)

    if args.merge:
        success = processor.merge_pdfs(args.merge, args.merge_output)
        sys.exit(0 if success else 1)

    # Process PDF
    options = {
        "output_format": args.format,
        "output_path": args.output,
        "ocr": args.ocr,
        "ocr_output_dir": args.ocr_dir,
        "extract_tables": args.extract_tables,
        "extract_images": args.extract_images,
        "detect_forms": args.detect_forms,
        "export_excel": args.export_excel,
        "excel_path": args.excel_path,
        "use_pdfplumber": args.use_pdfplumber
    }

    result = processor.process(options)

    # Print summary
    print(f"\nProcessing Summary:")
    print(f"- Pages: {result['metadata'].get('page_count', 'Unknown')}")
    print(f"- Text length: {len(result.get('text', ''))} characters")
    print(f"- Tables found: {len(result.get('tables', []))}")
    print(f"- Images found: {len(result.get('images', []))}")
    print(f"- Form fields: {len(result.get('forms', {}))}")

if __name__ == "__main__":
    import io  # For OCR image handling
    main()