"""Assemble the existing lecture files and record their hashes; no model calls."""
import datetime
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RECORDS = Path(__file__).resolve().parent
DEST = ROOT / "output/course-learning/charlie-20260906/清洗讲义"


def render():
    parts = json.loads((ROOT / "research/2026-09-06-charlie-course-organized/course-index.json").read_text())["parts"]
    chapters = []
    for lesson in range(1, 35):
        source = [p for p in parts if p["lesson"] == lesson]
        chapters.append({"file": f"第{lesson:02}课.md", "label": f"第{lesson:02}课 · {source[0]['title']}", "parts": [p["part"] for p in source], "record": f"lesson-{lesson:02}.json", "raw": f"正课-{lesson:02}.md"})
    for part in [1, *range(61, 93)]:
        source = next(p for p in parts if p["part"] == part)
        chapters.append({"file": f"补充-P{part:03}.md", "label": f"P{part:03} · {source['title']}", "parts": [part], "record": f"supplement-{part:03}.json", "raw": f"补充-P{part:03}.md"})
    for chapter in chapters:
        chapter["ready"] = (DEST / chapter["file"]).exists() and (RECORDS / chapter["record"]).exists()
    core = sum(c["ready"] for c in chapters[:34])
    supplements = sum(c["ready"] for c in chapters[34:])
    complete = all(c["ready"] for c in chapters)
    progress = {"updated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "status": "full_text_cleaning_first_pass_complete" if complete else "in_progress", "core_completed": core, "supplements_assembled": supplements, "native_chunks_completed": len(list((RECORDS / "native-supplements").glob("p???-c???-clean.json"))), "native_chunks_total": 163, "limits": ["Not full listening or visual verification", "Semantic review scopes are recorded separately; coverage is not an accuracy PASS"]}
    (RECORDS / "full-course-progress.json").write_text(json.dumps(progress, ensure_ascii=False, indent=2) + "\n")
    doc = ["# 查理的编剧课：全套连续清洗讲义", "", "按原课顺序保留讲解、案例的逐步发展、问答、改口、岔题与原作业。先把课程本身读完整，再做方法提炼。正文经机器稿清洗，**未逐句听校，非讲师逐字原话**。", "", f"目前已整理：**正课{core}/34，补充{supplements}/33**。全部92分集的[原始机器正文](../完整课程/README.md)保留，可逐处对照。", ""]
    if complete:
        doc += ["[整册连续讲义](课程清洗讲义-整册.md)汇集下列67章，适合从头阅读或保存；逐课阅读见下表。", ""]
    for label, group in [("正课1—34", chapters[:34]), ("补充专题：按公开分P顺序", chapters[34:])]:
        doc += [f"## {label}", "", "| 原课与标题 | 原分P | 阅读 |", "| --- | --- | --- |"]
        for c in group:
            link = f"[清洗讲义]({c['file']})" if c["ready"] else f"[机器底稿，清洗中](../完整课程/{c['raw']})"
            doc.append(f"| {c['label']} | {'、'.join(f'P{p:03}' for p in c['parts'])} | {link} |")
        doc.append("")
    doc += ["## 阅读与回查", "", "每节标出原P、时间与识别段号。段号连续只能证明已有机器段全部被分配，不能代替语义复核或逐句听校。原识别未覆盖的外语对白、画面、板书和表演不能靠清洗补齐。英文对白若译成中文，属于机器稿暂译；疑点保留。", "", "课中的行业、影片、人物、历史、技术、收入和招生说法按老师授课时的语境保留，未逐项外部核实；第一人称是讲述口吻。", "", "作业留在原来的课程节点，参照[原课作业索引](../原课作业.md)。课堂示范、旧题回收、可选练习和招生说明不改成新作业；第4课与第5课的要求分别保存。", "", "已有的[节点目录](../全套目录.md)、[跨课方法归纳](../课程核心方法论.md)与故事试跑继续保留。全文清洗的完成不代表Agent已学会写好故事或营销效果通过。", "", "[整理与复核记录](../../../../research/2026-09-08-charlie-course-cleaning/full-course-run.md) · [进度与范围](../../../../research/2026-09-08-charlie-course-cleaning/full-course-progress.json) · [返回课程总入口](../README.md)", ""]
    (DEST / "README.md").write_text("\n".join(doc))
    if complete:
        book = ["# 查理的编剧课：全套连续清洗讲义（整册）", "", "34课及33个补充专题，共67章。依据既有机器转录清洗；未逐句听校，非讲师逐字原话。正文中的疑点、讲师观点归属和各章整理限度一并保留。", "", "[逐课目录](README.md) · [完整机器底稿](../完整课程/README.md)", "", "## 目录", ""]
        for index, c in enumerate(chapters, 1):
            book.append(f"{index}. [{c['label']}](#chapter-{index:02})")
        book.append("")
        for index, c in enumerate(chapters, 1):
            body = (DEST / c["file"]).read_text()
            c["sha256"] = hashlib.sha256(body.encode()).hexdigest()
            book += ["---", "", f'<a id="chapter-{index:02}"></a>', "", body.rstrip(), ""]
        out = DEST / "课程清洗讲义-整册.md"
        out.write_text("\n".join(book))
        (RECORDS / "clean-book-manifest.json").write_text(json.dumps({"chapters": chapters, "book": str(out.relative_to(ROOT)), "book_sha256": hashlib.sha256(out.read_bytes()).hexdigest(), "limits": progress["limits"]}, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(progress, ensure_ascii=False))


if __name__ == "__main__":
    render()
