#!/usr/bin/env python3
"""
Arch HDL Spec — Content updater
Preserves ALL original docx formatting. Only splices in new sections from MD.

Usage:
  python3 update_arch_spec.py ARCH_HDL_Specification.md \
          ARCH_HDL_Specification_old.docx ARCH_HDL_Specification.docx
"""

import sys, re, zipfile, shutil, os, html



# ─── XML text escaping ────────────────────────────────────────────────────────
def esc(s):
    s = s.replace('\\<','<').replace('\\>','>') \
         .replace("\\'","'").replace('\\`','`') \
         .replace('\\\\','\\').replace('\\|','|') \
         .replace('\\-','-').replace('\\*','*') \
         .replace('\\[','[').replace('\\]',']') \
         .replace('\\!','!') \
         .replace('---','\u2014').replace('--','\u2013')
    return html.escape(s, quote=False)

# ─── XML run builder ──────────────────────────────────────────────────────────
def run(text, bold=False, italic=False, code=False,
        color="1E293B", sz=21, font="Arial"):
    if code:
        font="Courier New"; sz=17; color="569CD6"
    rpr = (f'<w:rFonts w:ascii="{font}" w:cs="{font}" w:eastAsia="{font}" w:hAnsi="{font}"/>'
           + ('<w:b/><w:bCs/>' if bold else '')
           + ('<w:i/><w:iCs/>' if italic else '')
           + f'<w:color w:val="{color}"/>'
           + f'<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/>')
    return f'<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{esc(text)}</w:t></w:r>'

def inline_runs(text, color="1E293B", sz=21, font="Arial"):
    runs = []
    for m in re.finditer(r'\*\*(.+?)\*\*|\*(.+?)\*|`([^`]+)`|([^*`]+)', text, re.DOTALL):
        if m.group(1): runs.append(run(m.group(1), bold=True,   color=color, sz=sz, font=font))
        elif m.group(2): runs.append(run(m.group(2), italic=True, color=color, sz=sz, font=font))
        elif m.group(3): runs.append(run(m.group(3), code=True))
        elif m.group(4): runs.append(run(m.group(4),             color=color, sz=sz, font=font))
    return ''.join(runs) if runs else run(text, color=color, sz=sz, font=font)

# ─── Cover page (table-based — renders correctly on Mac Pages/Preview) ────────
def xml_cover(title, subtitle, version_line, tagline):
    """Generate a cross-platform cover page.
    Uses table cell shading (works on Mac) instead of paragraph shading (Windows-only).
    """
    def cover_row(text, fill, color, sz, bold=False, before=0, after=0, center=True):
        jc = '<w:jc w:val="center"/>' if center else ''
        bold_tag = '<w:b/><w:bCs/>' if bold else '<w:b w:val="false"/><w:bCs w:val="false"/>'
        return (
            f'<w:tr><w:tc><w:tcPr>'
            f'<w:tcBorders>'
            f'<w:top w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:left w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:bottom w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:right w:val="none" w:color="auto" w:sz="0"/>'
            f'</w:tcBorders>'
            f'<w:shd w:fill="{fill}" w:val="clear"/>'
            f'<w:tcMar><w:top w:type="dxa" w:w="0"/><w:left w:type="dxa" w:w="360"/>'
            f'<w:bottom w:type="dxa" w:w="0"/><w:right w:type="dxa" w:w="360"/></w:tcMar>'
            f'</w:tcPr>'
            f'<w:p><w:pPr><w:spacing w:before="{before}" w:after="{after}"/>{jc}</w:pPr>'
            f'<w:r><w:rPr>'
            f'<w:rFonts w:ascii="Arial" w:cs="Arial" w:eastAsia="Arial" w:hAnsi="Arial"/>'
            f'{bold_tag}<w:color w:val="{color}"/>'
            f'<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/></w:rPr>'
            f'<w:t xml:space="preserve">{esc(text)}</w:t></w:r>'
            f'</w:p></w:tc></w:tr>'
        )

    def spacer_row(fill, pts):
        return (
            f'<w:tr><w:tc><w:tcPr>'
            f'<w:tcBorders>'
            f'<w:top w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:left w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:bottom w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:right w:val="none" w:color="auto" w:sz="0"/>'
            f'</w:tcBorders>'
            f'<w:shd w:fill="{fill}" w:val="clear"/>'
            f'<w:tcMar><w:top w:type="dxa" w:w="0"/><w:left w:type="dxa" w:w="0"/>'
            f'<w:bottom w:type="dxa" w:w="0"/><w:right w:type="dxa" w:w="0"/></w:tcMar>'
            f'</w:tcPr>'
            f'<w:p><w:pPr><w:spacing w:before="{pts}" w:after="0"/></w:pPr>'
            f'<w:r><w:rPr><w:sz w:val="4"/><w:szCs w:val="4"/></w:rPr><w:t> </w:t></w:r>'
            f'</w:p></w:tc></w:tr>'
        )

    rows = (
        spacer_row("1B2A47", 400)
        + cover_row(title,        "1B2A47", "FFFFFF", 96, bold=True,  before=0,   after=0)
        + cover_row(subtitle,     "1B2A47", "93C5FD", 30, bold=False, before=80,  after=0)
        + cover_row(version_line, "1B2A47", "64748B", 20, bold=False, before=120, after=0)
        + spacer_row("2563EB", 180)   # blue accent bar
    )

    header_tbl = (
        f'<w:tbl><w:tblPr><w:tblW w:type="dxa" w:w="9360"/>'
        f'<w:tblBorders>'
        f'<w:top w:val="none" w:color="auto" w:sz="0"/>'
        f'<w:left w:val="none" w:color="auto" w:sz="0"/>'
        f'<w:bottom w:val="none" w:color="auto" w:sz="0"/>'
        f'<w:right w:val="none" w:color="auto" w:sz="0"/>'
        f'</w:tblBorders></w:tblPr>'
        f'<w:tblGrid><w:gridCol w:w="9360"/></w:tblGrid>'
        + rows
        + '</w:tbl>'
    )

    tagline_text = re.sub(r'^\*+|\*+$', '', tagline).strip()
    tagline_para = (
        f'<w:p><w:pPr><w:spacing w:before="200" w:after="80"/><w:jc w:val="center"/></w:pPr>'
        + inline_runs(tagline_text, color="475569", sz=22)
        + '</w:p>'
    )
    return header_tbl + tagline_para + xml_gap(80)

# ─── Element XML generators ───────────────────────────────────────────────────
def xml_gap(n=60):
    return f'<w:p><w:pPr><w:spacing w:before="0" w:after="{n}"/></w:pPr><w:r><w:rPr><w:sz w:val="4"/><w:szCs w:val="4"/></w:rPr><w:t> </w:t></w:r></w:p>'

def xml_rule():
    return '<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:color="CBD5E1" w:sz="4" w:space="1"/></w:pBdr><w:spacing w:before="120" w:after="120"/></w:pPr></w:p>'

def xml_h1(text):
    return (f'<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:color="2563EB" w:sz="10" w:space="2"/></w:pBdr>'
            f'<w:spacing w:before="500" w:after="140"/></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Arial" w:cs="Arial" w:eastAsia="Arial" w:hAnsi="Arial"/>'
            f'<w:b/><w:bCs/><w:color w:val="1B2A47"/><w:sz w:val="36"/><w:szCs w:val="36"/></w:rPr>'
            f'<w:t xml:space="preserve">{esc(text)}</w:t></w:r></w:p>')

def xml_h2(text):
    return (f'<w:p><w:pPr><w:spacing w:before="320" w:after="80"/></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Arial" w:cs="Arial" w:eastAsia="Arial" w:hAnsi="Arial"/>'
            f'<w:b/><w:bCs/><w:color w:val="2563EB"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr>'
            f'<w:t xml:space="preserve">{esc(text)}</w:t></w:r></w:p>')

def xml_h3(text):
    return (f'<w:p><w:pPr><w:spacing w:before="200" w:after="60"/></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Arial" w:cs="Arial" w:eastAsia="Arial" w:hAnsi="Arial"/>'
            f'<w:b/><w:bCs/><w:color w:val="2563EB"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr>'
            f'<w:t xml:space="preserve">{esc(text)}</w:t></w:r></w:p>')

def xml_h4(text):
    return (f'<w:p><w:pPr><w:spacing w:before="160" w:after="40"/></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Arial" w:cs="Arial" w:eastAsia="Arial" w:hAnsi="Arial"/>'
            f'<w:b/><w:bCs/><w:color w:val="4B5563"/><w:sz w:val="21"/><w:szCs w:val="21"/></w:rPr>'
            f'<w:t xml:space="preserve">{esc(text)}</w:t></w:r></w:p>')

def xml_body(text):
    return (f'<w:p><w:pPr><w:spacing w:before="60" w:after="60"/></w:pPr>'
            + inline_runs(text) + '</w:p>')

def xml_bullet(text, level=0):
    indent = 720 + level * 360
    return (f'<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="1"/></w:numPr>'
            f'<w:spacing w:before="40" w:after="40"/><w:ind w:left="{indent}" w:hanging="360"/></w:pPr>'
            + inline_runs(text, sz=21) + '</w:p>')

def xml_numbered(text):
    return (f'<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr>'
            f'<w:spacing w:before="40" w:after="40"/><w:ind w:left="720" w:hanging="360"/></w:pPr>'
            + inline_runs(text, sz=21) + '</w:p>')

def xml_code_block(lines, lang='', label=''):
    is_sv   = lang in ('sv','systemverilog','verilog') or '.sv' in (label or '')
    bg      = "0D1117"
    lbdr    = "6B7280" if is_sv else "2563EB"
    lcolor  = "9CA3AF" if is_sv else "7C9CBF"
    tcolor  = "D1D5DB" if is_sv else "ADBAC7"
    lbl     = label or ('SystemVerilog' if is_sv else ('Arch' if lang=='arch' else lang or ''))

    rows = []
    if lbl:
        rows.append(
            f'<w:tr><w:tc><w:tcPr>'
            f'<w:tcBorders><w:top w:val="single" w:color="CBD5E1" w:sz="1"/>'
            f'<w:left w:val="single" w:color="{lbdr}" w:sz="18"/>'
            f'<w:bottom w:val="single" w:color="CBD5E1" w:sz="1"/>'
            f'<w:right w:val="single" w:color="CBD5E1" w:sz="1"/></w:tcBorders>'
            f'<w:shd w:fill="{bg}" w:val="clear"/>'
            f'<w:tcMar><w:top w:type="dxa" w:w="200"/><w:left w:type="dxa" w:w="280"/>'
            f'<w:bottom w:type="dxa" w:w="200"/><w:right w:type="dxa" w:w="280"/></w:tcMar>'
            f'</w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="36"/></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Courier New" w:cs="Courier New" w:eastAsia="Courier New" w:hAnsi="Courier New"/>'
            f'<w:i/><w:iCs/><w:color w:val="{lcolor}"/><w:sz w:val="17"/><w:szCs w:val="17"/></w:rPr>'
            f'<w:t xml:space="preserve">{esc(lbl)}</w:t></w:r></w:p></w:tc></w:tr>'
        )

    code_lines = lines if isinstance(lines, list) else (lines or '').split('\n')
    for line in code_lines:
        e = esc(line) if line.strip() else ' '
        rows.append(
            f'<w:tr><w:tc><w:tcPr>'
            f'<w:tcBorders><w:top w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:left w:val="single" w:color="{lbdr}" w:sz="18"/>'
            f'<w:bottom w:val="none" w:color="auto" w:sz="0"/>'
            f'<w:right w:val="none" w:color="auto" w:sz="0"/></w:tcBorders>'
            f'<w:shd w:fill="{bg}" w:val="clear"/>'
            f'<w:tcMar><w:top w:type="dxa" w:w="20"/><w:left w:type="dxa" w:w="280"/>'
            f'<w:bottom w:type="dxa" w:w="20"/><w:right w:type="dxa" w:w="280"/></w:tcMar>'
            f'</w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="2"/></w:pPr>'
            f'<w:r><w:rPr><w:rFonts w:ascii="Courier New" w:cs="Courier New" w:eastAsia="Courier New" w:hAnsi="Courier New"/>'
            f'<w:color w:val="{tcolor}"/><w:sz w:val="17"/><w:szCs w:val="17"/></w:rPr>'
            f'<w:t xml:space="preserve">{e}</w:t></w:r></w:p></w:tc></w:tr>'
        )

    tbl_xml = (
        f'<w:tbl><w:tblPr><w:tblW w:type="dxa" w:w="9360"/>'
        f'<w:tblBorders>'
        f'<w:top w:val="single" w:color="auto" w:sz="4"/>'
        f'<w:left w:val="single" w:color="auto" w:sz="4"/>'
        f'<w:bottom w:val="single" w:color="auto" w:sz="4"/>'
        f'<w:right w:val="single" w:color="auto" w:sz="4"/>'
        f'<w:insideH w:val="single" w:color="auto" w:sz="4"/>'
        f'<w:insideV w:val="single" w:color="auto" w:sz="4"/>'
        f'</w:tblBorders></w:tblPr>'
        f'<w:tblGrid><w:gridCol w:w="9360"/></w:tblGrid>'
        + ''.join(rows) + '</w:tbl>'
    )
    return [xml_gap(40), tbl_xml, xml_gap(40)]

def xml_data_table(headers, rows):
    n = max(len(headers), 1)
    col_w = 9360 // n
    cols = [col_w] * (n-1) + [9360 - col_w*(n-1)]

    grid = ''.join(f'<w:gridCol w:w="{c}"/>' for c in cols)

    def mk_cell(txt, is_hdr, ri, ci):
        bg = "1B2A47" if is_hdr else ("F1F5F9" if ri%2==1 else "FFFFFF")
        tc = "FFFFFF" if is_hdr else "1E293B"
        bold_tag = "<w:b/><w:bCs/>" if is_hdr else ""
        w = cols[min(ci, n-1)]
        return (f'<w:tc><w:tcPr><w:tcW w:type="dxa" w:w="{w}"/>'
                f'<w:tcBorders>'
                f'<w:top w:val="single" w:color="CBD5E1" w:sz="1"/>'
                f'<w:left w:val="single" w:color="CBD5E1" w:sz="1"/>'
                f'<w:bottom w:val="single" w:color="CBD5E1" w:sz="1"/>'
                f'<w:right w:val="single" w:color="CBD5E1" w:sz="1"/>'
                f'</w:tcBorders>'
                f'<w:shd w:fill="{bg}" w:val="clear"/>'
                f'<w:tcMar><w:top w:type="dxa" w:w="90"/><w:left w:type="dxa" w:w="140"/>'
                f'<w:bottom w:type="dxa" w:w="90"/><w:right w:type="dxa" w:w="140"/></w:tcMar>'
                f'</w:tcPr><w:p>'
                f'<w:r><w:rPr>'
                f'<w:rFonts w:ascii="Arial" w:cs="Arial" w:eastAsia="Arial" w:hAnsi="Arial"/>'
                f'{bold_tag}'
                f'<w:color w:val="{tc}"/><w:sz w:val="19"/><w:szCs w:val="19"/></w:rPr>'
                f'<w:t xml:space="preserve">{esc(txt)}</w:t></w:r></w:p></w:tc>')

    hdr_row = ('<w:tr><w:trPr><w:tblHeader/></w:trPr>'
               + ''.join(mk_cell(h, True, 0, ci) for ci,h in enumerate(headers))
               + '</w:tr>')
    data_rows = ''.join(
        '<w:tr><w:trPr><w:tblHeader w:val="false"/></w:trPr>'
        + ''.join(mk_cell(row[ci] if ci<len(row) else '', False, ri+1, ci)
                  for ci in range(n))
        + '</w:tr>'
        for ri, row in enumerate(rows)
    )
    return (f'<w:tbl><w:tblPr><w:tblW w:type="dxa" w:w="9360"/>'
            f'<w:tblBorders>'
            f'<w:top w:val="single" w:color="auto" w:sz="4"/>'
            f'<w:left w:val="single" w:color="auto" w:sz="4"/>'
            f'<w:bottom w:val="single" w:color="auto" w:sz="4"/>'
            f'<w:right w:val="single" w:color="auto" w:sz="4"/>'
            f'<w:insideH w:val="single" w:color="auto" w:sz="4"/>'
            f'<w:insideV w:val="single" w:color="auto" w:sz="4"/>'
            f'</w:tblBorders></w:tblPr>'
            f'<w:tblGrid>{grid}</w:tblGrid>'
            + hdr_row + data_rows + '</w:tbl>')

def xml_callout(text):
    return (f'<w:tbl><w:tblPr><w:tblW w:type="dxa" w:w="9360"/>'
            f'<w:tblBorders>'
            f'<w:top w:val="single" w:color="3B82F6" w:sz="4"/>'
            f'<w:left w:val="single" w:color="3B82F6" w:sz="12"/>'
            f'<w:bottom w:val="single" w:color="3B82F6" w:sz="4"/>'
            f'<w:right w:val="single" w:color="3B82F6" w:sz="4"/>'
            f'</w:tblBorders></w:tblPr>'
            f'<w:tblGrid><w:gridCol w:w="9360"/></w:tblGrid>'
            f'<w:tr><w:tc><w:tcPr>'
            f'<w:shd w:fill="EFF6FF" w:val="clear"/>'
            f'<w:tcMar><w:top w:type="dxa" w:w="160"/><w:left w:type="dxa" w:w="200"/>'
            f'<w:bottom w:type="dxa" w:w="160"/><w:right w:type="dxa" w:w="200"/></w:tcMar>'
            f'</w:tcPr><w:p>'
            + inline_runs(text.strip(), color="1E3A5F", sz=20)
            + '</w:p></w:tc></w:tr></w:tbl>')

# ─── MD parsers ───────────────────────────────────────────────────────────────
def unescape(s):
    return re.sub(r'\\([<>\'`\\|\-\*\[\]!@])', r'\1', s).replace('---','\u2014').replace('--','\u2013')

def detect_heading(line):
    line = line.strip()
    if not (line.startswith('**') and line.endswith('**')): return None
    inner = line[2:-2].strip()
    m = re.match(r'^(\d+(?:\.\d+)*[a-z]?)\.?\s+(.+)', inner)
    if m:
        parts = re.split(r'\.', re.sub(r'[a-z]$','',m.group(1)))
        return min(len(parts),4), inner
    if inner: return 2, inner
    return None

def parse_box_block(box_lines):
    inner = box_lines[1:-1]
    content = []
    for l in inner:
        m = re.match(r'^\|\s?(.*?)\s*\|?\s*$', l)
        content.append(m.group(1) if m else l)
    label, start = '', 0
    if content:
        first = next((c for c in content if c.strip()), '')
        if re.match(r'^\*[^*]+\*$', first.strip()):
            label = first.strip().strip('*')
            idx = content.index(first); start = idx+1
            while start < len(content) and not content[start].strip(): start += 1
    code_lines = []
    for l in content[start:]:
        l = re.sub(r'\*\*([^*]+)\*\*', r'\1', l)
        l = unescape(l)
        l = re.sub(r'\s*\|\s*$', '', l)
        code_lines.append(l)
    while code_lines and not code_lines[-1].strip(): code_lines.pop()
    is_sv = bool(re.search(r'\.sv\b', label, re.I))
    return label, code_lines, is_sv

def parse_pandoc_table(tbl_lines):
    is_sep = lambda l: bool(re.match(r'^\s{2,}(-+\s*)+$', l))
    sep_idx = [i for i,l in enumerate(tbl_lines) if is_sep(l)]
    if not sep_idx: return None, None
    sep = tbl_lines[sep_idx[0]]
    ranges = []
    in_d, start = False, 0
    for i,c in enumerate(sep):
        if c=='-' and not in_d: start=i; in_d=True
        elif c==' ' and in_d: ranges.append((start,i)); in_d=False
    if in_d: ranges.append((start,len(sep)))
    def ex(line): return [line[s:e].strip() for s,e in ranges]
    hdr_lines = [l for l in tbl_lines[:sep_idx[0]] if l.strip()]
    headers = ex(hdr_lines[-1]) if hdr_lines else []
    end = sep_idx[1] if len(sep_idx)>1 else len(tbl_lines)
    data = [ex(l) for l in tbl_lines[sep_idx[0]+1:end] if l.strip() and not is_sep(l)]
    clean = lambda s: re.sub(r'\*\*([^*]+)\*\*',r'\1',s).strip()
    return [clean(h) for h in headers], [[clean(c) for c in r] for r in data]

def parse_pipe_table(tbl_lines):
    def pr(l):
        p = l.strip().split('|')
        if p and p[0].strip()=='': p=p[1:]
        if p and p[-1].strip()=='': p=p[:-1]
        return [x.strip() for x in p]
    is_sep = lambda l: bool(re.match(r'^\|[\s\-:|]+\|$', l.strip()))
    non_sep = [l for l in tbl_lines if not is_sep(l)]
    if not non_sep: return None, None
    clean = lambda s: re.sub(r'\*\*([^*]+)\*\*',r'\1',s).strip().strip('`')
    headers = [clean(h) for h in pr(non_sep[0])]
    rows = [[clean(c) for c in pr(l)] for l in non_sep[1:]]
    return headers, rows

def parse_md_to_xml(md_text):
    lines = md_text.split('\n')
    out = []
    i = 0
    last_gap = False

    def push(*els):
        nonlocal last_gap
        for e in els: out.append(e); last_gap = False

    def push_gap(n=60):
        nonlocal last_gap
        if not last_gap: out.append(xml_gap(n)); last_gap = True

    while i < len(lines):
        line  = lines[i]
        strip = line.strip()

        if not strip:                           push_gap(80); i+=1; continue
        if re.search(r'<!--\s*page.?break\s*-->',strip,re.I):
            push('<w:p><w:r><w:br w:type="page"/></w:r></w:p>'); i+=1; continue
        if re.match(r'^---+$', strip):          push(xml_rule()); i+=1; continue

        # Fenced code
        if strip.startswith('```'):
            lang = strip[3:].strip().lower(); code=[]; i+=1
            while i<len(lines) and not lines[i].strip().startswith('```'):
                code.append(lines[i]); i+=1
            i+=1
            is_sv = lang in ('sv','systemverilog','verilog')
            lbl = 'SystemVerilog' if is_sv else ('Arch' if lang=='arch' else '')
            push_gap(40); push(*xml_code_block(code, lang=lang, label=lbl)); push_gap(40)
            continue

        # +---+ box
        if line.startswith('+--'):
            box=[line]; i+=1
            while i<len(lines):
                box.append(lines[i])
                if lines[i].startswith('+--'): i+=1; break
                i+=1
            lbl,code,is_sv = parse_box_block(box)
            push_gap(40); push(*xml_code_block(code, label=lbl, lang='sv' if is_sv else 'arch')); push_gap(40)
            continue

        # Pandoc simple table
        if re.match(r'^\s{2,}-{3,}', line):
            tbl=[line]; i+=1
            while i<len(lines):
                if re.match(r'^\s{2,}-{3,}', lines[i]): tbl.append(lines[i]); i+=1; break
                if not lines[i].strip(): break
                tbl.append(lines[i]); i+=1
            hdrs,rows = parse_pandoc_table(tbl)
            if hdrs is not None:
                push_gap(60); push(xml_data_table(hdrs, rows or [])); push_gap(60)
            continue

        # Pipe table
        if strip.startswith('|') and strip.endswith('|') and len(strip)>2:
            tbl=[]
            while i<len(lines) and lines[i].strip().startswith('|'):
                tbl.append(lines[i]); i+=1
            hdrs,rows = parse_pipe_table(tbl)
            if hdrs is not None:
                push_gap(60); push(xml_data_table(hdrs, rows or [])); push_gap(60)
            continue

        # Blockquote
        if line.startswith('> ') or line=='>':
            q=[]
            while i<len(lines) and (lines[i].startswith('> ') or lines[i]=='>'):
                q.append(lines[i][2:] if lines[i].startswith('> ') else ''); i+=1
            push_gap(60); push(xml_callout(' '.join(q))); push_gap(60)
            continue

        # **N. Heading**
        if strip.startswith('**') and strip.endswith('**') and len(strip)>4:
            h = detect_heading(strip)
            if h:
                lvl, text = h
                push_gap(40)
                push([xml_h1,xml_h2,xml_h3,xml_h4][lvl-1](text))
                i+=1; continue

        # Bullet
        bm = re.match(r'^(\s*)([-*+])\s+(.+)', line)
        if bm:
            push(xml_bullet(bm.group(3), min(len(bm.group(1))//2, 1))); i+=1; continue

        # Numbered
        nm = re.match(r'^\s*\d+\.\s+(.+)', line)
        if nm:
            push(xml_numbered(nm.group(1))); i+=1; continue

        # Body paragraph
        push(xml_body(strip)); i+=1

    return out

# ─── Helpers ──────────────────────────────────────────────────────────────────
def get_md_section(md_lines, start_pat, end_pat):
    start = None
    for i,l in enumerate(md_lines):
        if start is None and re.search(start_pat, l): start=i
        elif start is not None and re.search(end_pat, l): return md_lines[start:i]
    return md_lines[start:] if start is not None else []

def gen_xml(lines):
    return ''.join(parse_md_to_xml('\n'.join(lines)))

def splice_before(body, marker, new_xml):
    """Insert new_xml before the <w:p> that contains marker."""
    if not new_xml: return body
    idx = body.find(marker)
    if idx < 0:
        print(f"  Warning: splice marker not found: {marker!r}")
        return body
    para_start = body.rfind('<w:p>', 0, idx)
    if para_start < 0:
        print(f"  Warning: no <w:p> found before marker: {marker!r}")
        return body
    return body[:para_start] + new_xml + body[para_start:]

# ─── Main ─────────────────────────────────────────────────────────────────────
def main():
    if len(sys.argv) < 4:
        print("Usage: python3 update_arch_spec.py input.md old.docx output.docx")
        sys.exit(1)

    md_file, old_docx, out_docx = sys.argv[1], sys.argv[2], sys.argv[3]

    print(f"Reading {md_file}...")
    with open(md_file, encoding='utf-8') as f:
        md = f.read()
    md_lines = md.split('\n')

    print(f"Reading {old_docx}...")
    with zipfile.ZipFile(old_docx, 'r') as z:
        files = {n: z.read(n) for n in z.namelist()}

    # ── Work directly on raw XML bytes — no minidom, no pretty-printing ────
    raw = files['word/document.xml'].decode('utf-8')
    print(f"  Raw XML: {len(raw):,} chars")

    # Extract body content
    body_start = raw.index('<w:body>') + len('<w:body>')
    body_end   = raw.rindex('</w:body>')
    sect_start = raw.rindex('<w:sectPr', 0, body_end)
    sect_pr    = raw[sect_start:body_end]
    body       = raw[body_start:sect_start]
    prefix     = raw[:body_start]
    suffix     = raw[body_end:]
    print(f"  Body: {len(body):,} chars")

    # ── Replace cover page with fresh table-based version from MD ─────────
    # Extract cover fields from MD header (first ~10 lines)
    cover_lines = md_lines[:12]
    cover_title    = next((l.strip().strip('*') for l in cover_lines
                           if l.strip().startswith('**') and l.strip().endswith('**')
                           and 'Specification' not in l and 'Philosophy' not in l), 'ARCH')
    cover_subtitle = next((l.strip() for l in cover_lines
                           if l.strip() and not l.strip().startswith('*')
                           and 'Language Specification' not in l
                           and 'Design Philosophy' not in l), 'Hardware Description Language')
    cover_version  = next((l.strip() for l in cover_lines
                           if 'Language Specification' in l and not l.startswith('**')),
                          'Language Specification')
    cover_tagline  = next((l.strip() for l in cover_lines
                           if l.strip().startswith('*') and not l.strip().startswith('**')), '')
    new_cover = xml_cover(cover_title, cover_subtitle, cover_version, cover_tagline)

    # Find where the main body starts (first H1 heading "1.  Design Philosophy")
    # and replace everything before it with the fresh cover
    first_h1_marker = '>1.  Design Philosophy<'
    h1_idx = body.find(first_h1_marker)
    if h1_idx >= 0:
        para_start = body.rfind('<w:p>', 0, h1_idx)
        body = new_cover + body[para_start:]
        print(f"  Cover page: regenerated from MD ({cover_version})")
    else:
        print("  Warning: could not locate '1. Design Philosophy' to replace cover")

    # ── Targeted content patch: Reset ports row ────────────────────────────
    body = (body
        .replace('typed Reset&lt;Sync|Async&gt;',
                 'typed Reset&lt;Sync|Async, High|Low&gt;')
        .replace('>rst: in Reset&lt;Sync&gt;<',
                 '>rst: in Reset&lt;Sync&gt; (polarity defaults High; e.g. Reset&lt;Sync, Low&gt; for active-low)<'))

    # ── Generate new sections from MD ─────────────────────────────────────
    xml_421  = gen_xml(get_md_section(md_lines, r'^\*\*4\.2\.1\b',  r'^\*\*4\.3\b'))
    xml_53   = gen_xml(get_md_section(md_lines, r'^\*\*5\.3\b',     r'^\*\*6\b'))
    xml_72   = gen_xml(get_md_section(md_lines, r'^\*\*7\.2\b',     r'^\*\*8\b'))
    xml_82a  = gen_xml(get_md_section(md_lines, r'^\*\*8\.2a\b',    r'^\*\*9\b'))
    xml_1421 = gen_xml(get_md_section(md_lines, r'^\*\*14\.2\.1\b', r'^\*\*14\.3\b'))
    sec_29   = [l for l in get_md_section(md_lines, r'^\*\*29\b', r'XNEVER')
                if 'ARCH Language Specification' not in l]
    xml_29   = gen_xml(sec_29)

    print(f"  §4.2.1-4:{len(xml_421):,}  §5.3:{len(xml_53):,}  §7.2:{len(xml_72):,}"
          f"  §8.2a+8.3:{len(xml_82a):,}  §14.2.1:{len(xml_1421):,}  §29:{len(xml_29):,}")

    # ── Splice new sections in ─────────────────────────────────────────────
    body = splice_before(body, '>4.3  Module Instantiation<',           xml_421)
    body = splice_before(body, '>6.  First-Class Construct: pipeline<', xml_53)
    body = splice_before(body, '>8.  First-Class Construct: fifo<',     xml_72)
    body = splice_before(body, '>9.  First-Class Construct: arbiter<',  xml_82a)
    body = splice_before(body, '>14.3  Structural Variants<',           xml_1421)

    # Append §29 before footer
    footer = '>ARCH Language Specification v0.1<'
    footer_idx = body.rfind(footer)
    if footer_idx >= 0:
        para = body.rfind('<w:p>', 0, footer_idx)
        body = body[:para] + xml_29 + body[para:]
    else:
        body += xml_29

    # ── Rebuild document.xml ───────────────────────────────────────────────
    new_xml = (prefix + body + sect_pr + suffix).encode('utf-8')
    files['word/document.xml'] = new_xml
    print(f"  New document.xml: {len(new_xml):,} bytes")

    # ── Write output docx ──────────────────────────────────────────────────
    print(f"Writing {out_docx}...")
    shutil.copy2(old_docx, out_docx)
    with zipfile.ZipFile(out_docx, 'w', zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            z.writestr(name, data)

    print(f"✓ Done: {out_docx} ({os.path.getsize(out_docx)//1024} KB)")

if __name__ == '__main__':
    main()
