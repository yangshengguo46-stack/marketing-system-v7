from pathlib import Path
import re
import html
import json
import hashlib
from urllib.parse import quote

from reportlab.pdfgen import canvas
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.lib import colors
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.enums import TA_LEFT
from reportlab.platypus import (
    BaseDocTemplate, PageTemplate, Frame, Paragraph, Spacer, PageBreak,
    Table, TableStyle, Flowable, KeepTogether,
)
from reportlab.platypus.tableofcontents import TableOfContents
from pypdf import PdfReader

ROOT = Path(__file__).resolve().parents[2]
RESEARCH = Path(__file__).resolve().parent
SOURCE = RESEARCH / 'report-source.md'
OUTPUT = ROOT / 'output' / 'pdf' / 'Codex-IP运营Agent-深度研究-2026-09-05.pdf'
OUTPUT.parent.mkdir(parents=True, exist_ok=True)
QA = RESEARCH / 'qa'
QA.mkdir(exist_ok=True)

pdfmetrics.registerFont(TTFont('CJK', '/System/Library/Fonts/STHeiti Light.ttc', subfontIndex=0))
pdfmetrics.registerFont(TTFont('CJKBold', '/System/Library/Fonts/STHeiti Medium.ttc', subfontIndex=0))
pdfmetrics.registerFontFamily('CJK', normal='CJK', bold='CJKBold', italic='CJK', boldItalic='CJKBold')

NAVY = colors.HexColor('#183A45')
TEAL = colors.HexColor('#187D80')
INK = colors.HexColor('#263C45')
MUTED = colors.HexColor('#63777E')
LINE = colors.HexColor('#D4E1E3')
PALE = colors.HexColor('#F2F7F7')
AMBER = colors.HexColor('#CF924B')
W, H = 595.276, 841.890
MARGIN = 49
CW = W - 2 * MARGIN

styles = {
    'body': ParagraphStyle('body', fontName='CJK', fontSize=10.4, leading=17.4,
        textColor=INK, wordWrap='CJK', spaceAfter=9, splitLongWords=True),
    'chapter': ParagraphStyle('chapter', fontName='CJKBold', fontSize=20.5,
        leading=30, textColor=NAVY, spaceAfter=18, keepWithNext=True, wordWrap='CJK'),
    'small': ParagraphStyle('small', fontName='CJK', fontSize=8.5, leading=13,
        textColor=MUTED, spaceAfter=8, wordWrap='CJK'),
    'bullet': ParagraphStyle('bullet', fontName='CJK', fontSize=10.4, leading=17.4,
        leftIndent=11, firstLineIndent=-9, textColor=INK, spaceAfter=7, wordWrap='CJK'),
    'cell': ParagraphStyle('cell', fontName='CJK', fontSize=9, leading=14.2,
        textColor=INK, wordWrap='CJK', spaceAfter=0),
    'cellhead': ParagraphStyle('cellhead', fontName='CJKBold', fontSize=9.2, leading=14.2,
        textColor=colors.white, wordWrap='CJK', spaceAfter=0),
    'toc': ParagraphStyle('toc', fontName='CJK', fontSize=11, leading=18,
        textColor=INK, spaceBefore=8, spaceAfter=5, wordWrap='CJK'),
    'cover': ParagraphStyle('cover', fontName='CJKBold', fontSize=30, leading=43,
        textColor=NAVY, spaceAfter=23, wordWrap='CJK'),
    'subtitle': ParagraphStyle('subtitle', fontName='CJK', fontSize=16, leading=26,
        textColor=TEAL, spaceAfter=30, wordWrap='CJK'),
}

LINK_RE = re.compile(r'\[([^\]]+)\]\((?:<([^>]+)>|([^\s)]+))\)')

def inline(text):
    fragments = []
    last = 0
    for match in LINK_RE.finditer(text):
        fragments.append(html.escape(text[last:match.start()]))
        url = match.group(2) or match.group(3)
        label = html.escape(match.group(1))
        fragments.append(f'<a href="{html.escape(url, quote=True)}" color="#187D80">{label}</a>')
        last = match.end()
    fragments.append(html.escape(text[last:]))
    value = ''.join(fragments)
    value = re.sub(r'\*\*(.+?)\*\*', r'<b>\1</b>', value)
    value = re.sub(r'`([^`]+)`', r'<font color="#536D74">\1</font>', value)
    return value

class OperatingLoop(Flowable):
    def __init__(self):
        Flowable.__init__(self)
        self.width = CW
        self.height = 224

    def draw(self):
        c = self.canv
        bw, bh, gap = (CW - 40) / 3, 63, 20
        rows = [
            [('目标与现实材料', '主体 · 受众 · 行动'), ('研究与取舍', '证据 · 创意 · 选择'), ('制作与交付', '脚本 · 媒体 · 版本')],
            [('下一轮 Mission', '新问题 · 新材料 · 新行动'), ('复盘与项目经验', '观察 · 假设 · 修订'), ('实际发布与数据', '最终版本 · 反馈 · 成本')],
        ]
        for ridx, row in enumerate(rows):
            y = 138 if ridx == 0 else 42
            for idx, (title, sub) in enumerate(row):
                x = idx * (bw + gap)
                c.setFillColor(PALE)
                c.setStrokeColor(LINE)
                c.roundRect(x, y, bw, bh, 8, fill=1, stroke=1)
                c.setFillColor(NAVY)
                c.setFont('CJKBold', 12)
                c.drawCentredString(x + bw / 2, y + 37, title)
                c.setFont('CJK', 8.8)
                c.setFillColor(MUTED)
                c.drawCentredString(x + bw / 2, y + 17, sub)
        def arrow(x1,y1,x2,y2):
            c.setStrokeColor(TEAL); c.setLineWidth(1.2)
            c.line(x1,y1,x2,y2)
            import math
            angle = math.atan2(y2-y1,x2-x1)
            for delta in (-.55,.55):
                c.line(x2,y2,x2-5*math.cos(angle+delta),y2-5*math.sin(angle+delta))
        for idx in range(2):
            arrow((idx+1)*bw+idx*gap+3,169,(idx+1)*(bw+gap)-3,169)
            arrow((idx+1)*(bw+gap)-3,73,(idx+1)*bw+idx*gap+3,73)
        arrow(CW-bw/2,135,CW-bw/2,108)
        arrow(bw/2,108,bw/2,135)
        c.setFont('CJK',9); c.setFillColor(TEAL)
        c.drawCentredString(CW/2,14,'共同依据：当前项目版本、实际证据、授权范围与可用资源')

class ResearchDoc(BaseDocTemplate):
    def __init__(self, filename):
        super().__init__(str(filename), pagesize=(W,H), leftMargin=MARGIN,
            rightMargin=MARGIN, topMargin=62, bottomMargin=51,
            title='以 Codex 为底座，做出真正能持续工作的 IP 运营 Agent',
            author='Codex · 项目研究', subject='第七版北极星、历史Git与台账、跨学科研究和开源方案')
        self.chapters = []
        self.chapter_title = ''
        frame = Frame(MARGIN, 51, CW, H-113, leftPadding=0, rightPadding=0,
            topPadding=0, bottomPadding=0, id='body')
        self.addPageTemplates(PageTemplate(id='normal', frames=frame, onPage=self.decorate))

    def decorate(self,c,doc):
        c.saveState()
        if doc.page == 1:
            c.setFillColor(TEAL); c.rect(0,H-12,W,12,fill=1,stroke=0)
        else:
            c.setFont('CJK',8); c.setFillColor(MUTED)
            c.drawString(MARGIN,H-31,'CODEX / IP 运营 AGENT / 深度研究')
            c.drawRightString(W-MARGIN,H-31,'2026.09.05')
            c.setStrokeColor(LINE); c.line(MARGIN,H-42,W-MARGIN,H-42)
        c.setStrokeColor(LINE); c.line(MARGIN,38,W-MARGIN,38)
        c.setFillColor(MUTED); c.setFont('CJK',8)
        c.drawString(MARGIN,24,'基于第七版北极星 · 实现建议与证据边界')
        c.drawRightString(W-MARGIN,24,f'{doc.page:02d}')
        c.restoreState()

    def afterFlowable(self, flowable):
        if isinstance(flowable, Paragraph) and flowable.style.name == 'chapter':
            plain=flowable.getPlainText()
            key='chapter-'+plain.split(' ',1)[0]
            self.canv.bookmarkPage(key)
            self.canv.addOutlineEntry(plain,key,0,False)
            self.notify('TOCEntry',(0,plain,self.page,key))

source=SOURCE.read_text()
source=source.replace('\u2011','-').replace('\u2013','-').replace('\u2014','-')
sections=re.split(r'^## ',source,flags=re.M)
story=[]
story.append(Spacer(1,41))
story.append(Paragraph('研究报告 / RESEARCH REPORT',styles['small']))
story.append(Spacer(1,20))
story.append(Paragraph('以 Codex 为底座，<br/>做出真正能持续工作的<br/>IP 运营 Agent',styles['cover']))
story.append(Paragraph('第七版北极星下的实现研究',styles['subtitle']))
coverbox=Table([[Paragraph('持续产出可用内容<br/>建立影响力<br/>带来可衡量的关注、信任或业务行动',
    ParagraphStyle('coverbox',parent=styles['body'],fontSize=12,leading=23,textColor=NAVY))]],colWidths=[CW])
coverbox.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,-1),PALE),('BOX',(0,0),(-1,-1),.5,LINE),
    ('LEFTPADDING',(0,0),(-1,-1),18),('RIGHTPADDING',(0,0),(-1,-1),18),
    ('TOPPADDING',(0,0),(-1,-1),14),('BOTTOMPADDING',(0,0),(-1,-1),14)]))
story.append(coverbox); story.append(Spacer(1,34))
for text in [
    '2026 年 9 月 5 日',
    '面向产品负责人、业务负责人及研发团队',
    '结合第七版规格、前几版历史台账与关键 Git 提交、原始论文及官方开源实现。',
    '本报告提出实现与验证建议。没有修改产品代码或现役计划，没有运行新的付费模型实验，也不宣称已证明长期业务效果。',
]: story.append(Paragraph(text,styles['small']))
story.append(PageBreak())
story.append(Paragraph('阅读导航',ParagraphStyle('navigation',parent=styles['chapter'])))
story.append(Paragraph('先读第 01、02、14 节判断方向与下一步；第 03、04 节核对历史和现状；第 05 至 13 节说明具体能力、开源取舍和验证方式。',styles['body']))
toc=TableOfContents(); toc.levelStyles=[styles['toc']];toc.dotsMinLevel=0
story.append(toc)

def add_table(lines, compact=False):
    rows=[]
    for line in lines:
        cells=[c.strip() for c in line.strip().strip('|').split('|')]
        if all(re.fullmatch(r':?-+:?',c) for c in cells):continue
        rows.append(cells)
    n=len(rows[0])
    widths=[CW*.235,CW*.43,CW*.335] if n==3 else [CW/n]*n
    cell_style = ParagraphStyle('compact_cell',parent=styles['cell'],leading=13) if compact else styles['cell']
    table=Table([[Paragraph(inline(cell).replace('；','<br/>') if cidx==0 and ridx>0 else inline(cell),styles['cellhead'] if ridx==0 else cell_style)
        for cidx,cell in enumerate(row)] for ridx,row in enumerate(rows)],colWidths=widths,repeatRows=1,hAlign='LEFT')
    cmds=[('BACKGROUND',(0,0),(-1,0),NAVY),('VALIGN',(0,0),(-1,-1),'TOP'),
        ('LEFTPADDING',(0,0),(-1,-1),9),('RIGHTPADDING',(0,0),(-1,-1),9),
        ('TOPPADDING',(0,0),(-1,-1),6 if compact else 9),('BOTTOMPADDING',(0,0),(-1,-1),6 if compact else 9),
        ('LINEBELOW',(0,0),(-1,0),.5,NAVY),('LINEBELOW',(0,1),(-1,-1),.4,LINE)]
    for ridx in range(1,len(rows)):
        if ridx%2:cmds.append(('BACKGROUND',(0,ridx),(-1,ridx),PALE))
    table.setStyle(TableStyle(cmds));story.append(table);story.append(Spacer(1,14))

for section in sections[1:]:
    lines=section.strip().splitlines()
    story.append(PageBreak())
    story.append(Paragraph(inline(lines[0]),styles['chapter']))
    i=1
    while i<len(lines):
        line=lines[i].strip()
        if not line:i+=1;continue
        if line=='[[OPERATING_LOOP]]':story.append(OperatingLoop());i+=1;continue
        if line.startswith('|'):
            tablelines=[]
            while i<len(lines) and lines[i].strip().startswith('|'):
                tablelines.append(lines[i]);i+=1
            add_table(tablelines,compact=lines[0].startswith('03 '));continue
        if line.startswith('- '):
            story.append(Paragraph('• '+inline(line[2:]),styles['bullet']));i+=1;continue
        texts=[line];i+=1
        while i<len(lines) and lines[i].strip() and not lines[i].strip().startswith(('|','- ','[[')):
            texts.append(lines[i].strip());i+=1
        style=styles['small'] if texts[0].startswith(('证据入口：','关键代码：','来源：')) or lines[0].startswith('16 ') else styles['body']
        story.append(Paragraph(inline(' '.join(texts)),style))

doc=ResearchDoc(OUTPUT)
doc.multiBuild(story)
reader=PdfReader(str(OUTPUT))
pages=[]
for i,page in enumerate(reader.pages):
    text=page.extract_text() or ''
    links=[]
    for ann in page.get('/Annots',[]):
        obj=ann.get_object()
        if '/A' in obj and '/URI' in obj['/A']:
            links.append(str(obj['/A']['/URI']))
    pages.append({'page':i+1,'chars':len(text),'links':links,'text':text})
qa={'file':str(OUTPUT),'pages':len(pages),'bytes':OUTPUT.stat().st_size,
    'sha256':hashlib.sha256(OUTPUT.read_bytes()).hexdigest(),
    'page_records':pages,'visual_review':'pending'}
(QA/'structural.json').write_text(json.dumps(qa,ensure_ascii=False,indent=2))
print(json.dumps({k:v for k,v in qa.items() if k!='page_records'},ensure_ascii=False,indent=2))
for p in pages:
    print(p['page'],p['chars'],len(p['links']),p['text'].splitlines()[3:5])
