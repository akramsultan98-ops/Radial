"""Regenerate the PDF, Word and text test fixtures
(requires reportlab, pypdf, python-docx, Pillow).

    python3 make_fixtures.py

Output is deterministic (reportlab invariant mode) so diffs stay meaningful.
"""

import io
import math
from pathlib import Path

from PIL import Image
from pypdf import PdfReader, PdfWriter
from reportlab import rl_config
from reportlab.lib.pagesizes import A4
from reportlab.lib.utils import ImageReader
from reportlab.pdfgen import canvas

HERE = Path(__file__).parent
# Store the photo as a plain DCTDecode stream, as most PDF producers do.
rl_config.useA85 = 0


def photo() -> ImageReader:
    w, h = 360, 240
    img = Image.new("RGB", (w, h))
    px = img.load()
    for y in range(h):
        for x in range(w):
            r = int(128 + 120 * math.sin(x / 23.0))
            g = int(128 + 120 * math.sin(y / 17.0 + x / 41.0))
            b = (x * 7 + y * 13) % 256
            px[x, y] = (r, g, b)
    buf = io.BytesIO()
    img.save(buf, "JPEG", quality=92)
    buf.seek(0)
    return ImageReader(buf)


def report() -> None:
    path = HERE / "report.pdf"
    c = canvas.Canvas(str(path), pagesize=A4, invariant=1, pageCompression=0)
    c.setTitle("Quarterly report")
    c.setAuthor("Old author")
    c.setSubject("Account review")
    c.setKeywords("finance, q3")
    w, h = A4
    c.setFont("Helvetica-Bold", 20)
    c.drawString(72, h - 90, "Quarterly report")
    c.setFont("Helvetica", 12)
    c.drawString(72, h - 130, "Client: ACME Corporation")
    c.drawString(72, h - 150, "IBAN DE89 3704 0044 0532 0130 00")
    c.drawString(72, h - 170, "Contact: jane@example.com")
    c.setFillColorRGB(0.2, 0.4, 0.9)
    c.rect(72, h - 230, 200, 30, fill=1, stroke=0)
    c.drawImage(photo(), 72, h - 560, width=300, height=200)
    c.showPage()
    c.setFont("Helvetica", 14)
    c.drawString(72, h - 100, "Second page secret: ZEBRA-42")
    c.drawString(72, h - 130, "Public footer text")
    c.showPage()
    c.save()
    # Display page 2 turned a quarter clockwise (/Rotate 90) so tests cover
    # rotated pages: its portrait content appears landscape.
    reader = PdfReader(path)
    out = PdfWriter(clone_from=reader)
    out.pages[1].rotate(90)
    with open(path, "wb") as f:
        out.write(f)


def bookmarked() -> None:
    # A bookmark title holds text that is not on any page.
    buf = io.BytesIO()
    c = canvas.Canvas(buf, pagesize=A4, invariant=1)
    c.drawString(72, 700, "Contract overview")
    c.showPage()
    c.drawString(72, 700, "Payment to account VAULT-7731")
    c.save()
    buf.seek(0)
    out = PdfWriter(clone_from=PdfReader(buf))
    out.add_outline_item("Notes for Project Nightjar", 0)
    out.add_outline_item("VAULT-7731 details", 1)
    with open(HERE / "bookmarked.pdf", "wb") as f:
        out.write(f)


def encrypted() -> None:
    buf = io.BytesIO()
    c = canvas.Canvas(buf, pagesize=A4, invariant=1)
    c.drawString(72, 700, "Locked")
    c.save()
    buf.seek(0)
    out = PdfWriter()
    for page in PdfReader(buf).pages:
        out.add_page(page)
    out.encrypt(user_password="secret", owner_password="owner", algorithm="AES-128")
    with open(HERE / "encrypted.pdf", "wb") as f:
        out.write(f)
    # Opens without a password but restricts editing (owner password only).
    buf.seek(0)
    out = PdfWriter()
    for page in PdfReader(buf).pages:
        out.add_page(page)
    out.encrypt(user_password="", owner_password="owner", algorithm="AES-128", permissions_flag=4)
    with open(HERE / "restricted.pdf", "wb") as f:
        out.write(f)


def memo() -> None:
    import datetime

    import docx

    d = docx.Document()
    d.add_heading("Client memo", level=1)
    d.add_paragraph("Account: ACME Corporation\tTier 1")
    p = d.add_paragraph("Renewal due ")
    p.add_run("31 March").bold = True
    p.add_run(" — budget & scope agreed.")
    t = d.add_table(rows=1, cols=2)
    t.rows[0].cells[0].text = "Contact"
    t.rows[0].cells[1].text = "Jane Doe"
    cp = d.core_properties
    cp.title = "Client memo"
    cp.author = "Old author"
    cp.last_modified_by = "Jane Doe"
    cp.subject = "Renewal"
    cp.keywords = "acme, renewal"
    cp.created = datetime.datetime(2024, 1, 31, 14, 30)
    cp.modified = datetime.datetime(2024, 2, 1, 9, 0)
    cp.revision = 3
    d.save(HERE / "memo.docx")


def texts() -> None:
    lines = [f"Line {i:03d}: the quick brown fox jumps over the lazy dog." for i in range(1, 121)]
    lines.insert(5, "A very long line " + "that keeps going " * 20 + "END")
    (HERE / "notes.txt").write_text("\n".join(lines) + "\n\x0cAfter the page break\n", encoding="utf-8")
    (HERE / "greek.txt").write_text("Καλημέρα κόσμε\nΠρόγραμμα συνάντησης\n", encoding="utf-8")


if __name__ == "__main__":
    report()
    bookmarked()
    encrypted()
    memo()
    texts()
