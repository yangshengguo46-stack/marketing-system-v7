"""Assemble local course notes from reviewed cards and completed raw ASR files."""

import csv
import io
import json
from pathlib import Path
import re
import shutil

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
VAULT = ROOT / "output/course-learning/charlie-20260906"


def write_note(path, properties, body):
    path.parent.mkdir(parents=True, exist_ok=True)
    header = "\n".join(f"{key}: {json.dumps(value, ensure_ascii=False)}" for key, value in properties.items())
    path.write_text(f"---\n{header}\n---\n\n{body.rstrip()}\n", encoding="utf-8")


def clock(seconds):
    hours, seconds = divmod(round(seconds), 3600)
    minutes, seconds = divmod(seconds, 60)
    return f"{hours:02d}:{minutes:02d}:{seconds:02d}"


def include_review_assets(body, source_note):
    def replace(match):
        label, target = match.groups()
        if "://" in target or target.startswith("#"):
            return match.group(0)
        source = (source_note.parent / target).resolve()
        transcript_match = re.fullmatch(r"p(\d+)\.source-time\.txt", source.name)
        if source.parent == HERE / "transcripts" and transcript_match:
            part = int(transcript_match.group(1))
            return f"[{label}](../转录/P{part:03d}.md)"
        if not source.is_file() or not source.is_relative_to(HERE / "reviews"):
            return match.group(0)
        lesson_match = re.fullmatch(r"lesson-(\d+)\.md", source.name)
        if lesson_match:
            lesson = int(lesson_match.group(1))
            return f"[{label}](../课程/第{lesson:02d}课.md)"
        supplement_match = re.fullmatch(r"p(\d+)\.md", source.name)
        if supplement_match:
            part = int(supplement_match.group(1))
            return f"[{label}](../补充/P{part:03d}.md)"
        relative = source.relative_to(HERE / "reviews")
        destination = VAULT / "资料附件" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        return f"[{label}](../资料附件/{relative.as_posix()})"

    return re.sub(r"\[([^\]]+)\]\(([^)]+)\)", replace, body)


def main():
    catalog = json.loads((HERE / "course-index.json").read_text())
    parts = catalog["parts"]
    source_by_part = {row["part"]: row for row in parts}
    transcribed = {}
    reviewed = {}
    supplements = {}
    assignments = []
    for path in sorted((HERE / "transcripts").glob("p???.json")):
        data = json.loads(path.read_text())
        if data.get("state") != "completed":
            continue
        part = data["part"]
        assert data["source"]["sha256"] == source_by_part[part]["sha256"]
        transcribed[part] = data
        body = [f"# P{part:03d} · {data['title']}", "", "机器转录，未逐句听校。时间指向原视频，不表示文字已经准确。",
                f"原课：[视频入口]({source_by_part[part]['url']})；CID：{source_by_part[part]['cid']}。", ""]
        body.extend(f"[{clock(s['source_start_seconds'])}–{clock(s['source_end_seconds'])}] {s['text']}"
                    for s in data["segments"])
        write_note(VAULT / "转录" / f"P{part:03d}.md", {"type": "raw-transcript", "part": part, "status": "机器稿未逐句听校"}, "\n\n".join(body))
    for path in sorted((HERE / "reviews").glob("lessons-*/*.json")):
        md = path.with_suffix(".md")
        data = json.loads(path.read_text())
        if not md.exists() or not data.get("full_transcript_read_parts"):
            continue
        lesson = data["lesson"]
        expected = [row["part"] for row in parts if row["lesson"] == lesson]
        if sorted(data["full_transcript_read_parts"]) != expected:
            continue
        reviewed[lesson] = data
        body = include_review_assets(md.read_text(), md)
        body += "\n\n## 本资料包中的转录\n\n"
        if (
            lesson == 2
            and "whisper" in data.get("transcript_source", "")
            and data.get("current_batch_full_transcript_read_parts") != expected
        ):
            body += "本课全文核读基底是前轮 Whisper 机器稿："
            for part in expected:
                original = ROOT / f"research/2026-09-06-course-to-capability/asr/p{part:03d}.srt"
                destination = VAULT / "转录" / f"旧版-P{part:03d}-whisper.srt"
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(original, destination)
                body += f"[P{part:03d} 核读原稿](../转录/{destination.name})；"
            body += "\n\n以下为新批次 SenseVoice 机器稿，不冒称本卡已经全文重读新版：\n\n"
        body += "、".join(f"[P{part:03d}](../转录/P{part:03d}.md)" for part in expected if part in transcribed)
        if not any(part in transcribed for part in expected):
            body += "新批次转录尚未全部就绪；本课核读所用旧转录来源见上文。"
        write_note(VAULT / "课程" / f"第{lesson:02d}课.md", {"type": "lesson", "lesson": lesson, "parts": expected,
                   "status": "全文机器稿已核读，未逐句听校"}, body)
        for item in data.get("assignments", []):
            assignments.append({"lesson": lesson, **item})
    for path in sorted((HERE / "reviews").glob("supplements-*/*.json")):
        md = path.with_suffix(".md")
        data = json.loads(path.read_text())
        if not md.exists() or not data.get("full_transcript_read_parts"):
            continue
        part = data["part"]
        if data["full_transcript_read_parts"] != [part]:
            continue
        supplements[part] = data
        body = include_review_assets(md.read_text(), md)
        if part in transcribed:
            body += f"\n\n## 本资料包中的转录\n\n[P{part:03d} 完整机器稿](../转录/P{part:03d}.md)\n"
        write_note(VAULT / "补充" / f"P{part:03d}.md", {"type": "supplement", "part": part,
                   "status": "全文机器稿已核读，未逐句听校"}, body)
        for item in data.get("assignments", []):
            assignments.append({"supplement": part, "part": part, **item})
    for item in assignments:
        if item.get("participation") == "optional_open_call":
            item["original_kind"] = item["kind"]
            item["kind"] = "optional_open_call"
    complete_review = len(reviewed) == 34 and len(supplements) == 33
    for row in parts:
        row["transcript_status"] = "machine_transcribed" if row["part"] in transcribed else "pending_new_asr"
        is_reviewed = row["lesson"] in reviewed if row["lesson"] is not None else row["part"] in supplements
        row["review_status"] = "full_machine_transcript_read_not_full_listen" if is_reviewed else "pending"
    index_path = HERE / "course-index.json"
    index_tmp = index_path.with_suffix(".json.tmp")
    index_tmp.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n")
    index_tmp.replace(index_path)
    csv_buffer = io.StringIO(newline="")
    csv_writer = csv.DictWriter(csv_buffer, fieldnames=list(parts[0]))
    csv_writer.writeheader()
    csv_writer.writerows(parts)
    csv_path = HERE / "course-index.csv"
    csv_tmp = csv_path.with_suffix(".csv.tmp")
    csv_tmp.write_text(csv_buffer.getvalue())
    csv_tmp.replace(csv_path)
    core_lines = ["# 全套课程目录", "", "按老师原编号阅读。原编号课程有34课、59分集；33个补充分集同样保留。正文状态以逐课核读为准。", "",
                  f"当前完成：机器转录 {len(transcribed)}/92 分集；原编号课程核读 {len(reviewed)}/34 课；补充分集核读 {len(supplements)}/33。核读指完整机器稿阅读与课程整理，不代表逐句听校。", "",
                  "## 原编号课程", "", "| 课次 | 原课题目 | 分集与原视频 | 时长 | 正文 |", "| --- | --- | --- | --- | --- |"]
    for lesson in range(1, 35):
        group = [row for row in parts if row["lesson"] == lesson]
        source_links = "、".join(f"[P{row['part']:03d}]({row['url']})" for row in group)
        state = f"[课程页](课程/第{lesson:02d}课.md)" if lesson in reviewed else "待核读"
        core_lines.append(f"| {lesson:02d} | {group[0]['title']} | {source_links} | {clock(sum(row['duration_seconds'] for row in group))} | {state} |")
    core_lines += ["", "## 补充专题与拉片", "", "| 分集 | 原题 | 时长 | 转录 | 正文 |", "| --- | --- | --- | --- | --- |"]
    for row in parts:
        if row["lesson"] is not None:
            continue
        part = row["part"]
        state = f"[机器稿](转录/P{part:03d}.md)" if part in transcribed else "处理中"
        card = f"[课程页](补充/P{part:03d}.md)" if part in supplements else "待核读"
        core_lines.append(f"| [P{part:03d}]({row['url']}) | {row['title']} | {clock(row['duration_seconds'])} | {state} | {card} |")
    write_note(VAULT / "全套目录.md", {"type": "course-map", "status": "全套机器稿已核读" if complete_review else "正文整理中"}, "\n".join(core_lines))
    homework = ["# 原课作业与练习", "", "仅收录已全文核读课程里实际找到的要求。空白未处理课程不能据此推定没有作业；课堂练习和前课作业回顾分开记录。同一作业在多课重述会有多条来源证据，不表示每条都要新写一份。", "", "录课时的征稿、提交入口和讲评安排按历史语境保留，尚未确认目前仍开放。", ""]
    labels = [("explicit_assignment", "老师明确布置的作业"), ("classroom_exercise", "课堂练习与持续训练建议"), ("prior_assignment_recap", "前课作业的回收与后续使用"), ("optional_open_call", "录课时的自愿征稿与讲评活动")]
    known_kinds = {kind for kind, _label in labels}
    unknown_kinds = {item.get("kind") for item in assignments} - known_kinds
    if unknown_kinds:
        raise ValueError(f"Assignment kinds need source review before indexing: {unknown_kinds}")
    for kind, label in labels:
        homework += [f"## {label}", "", "| 来源课 | 实际要求 | 原视频时间 | 交付要求 | 后续讲评 / 使用 |", "| --- | --- | --- | --- | --- |"]
        for item in sorted(assignments, key=lambda item: (item.get("supplement") is not None, item.get("lesson", item.get("supplement", 0)), item.get("part", 0), item.get("start_seconds", 0))):
            if item.get("kind") != kind:
                continue
            part = item["part"]
            url = source_by_part[part]["url"].rstrip("/")
            url += ("&" if "?" in url else "?") + f"t={int(item['start_seconds'])}"
            if "lesson" in item:
                origin = f"[第{item['lesson']:02d}课](课程/第{item['lesson']:02d}课.md)"
            else:
                origin = f"[补充 P{item['supplement']:03d}](补充/P{item['supplement']:03d}.md)"
            cells = [origin, item["task"],
                     f"[P{part:03d} {clock(item['start_seconds'])}]({url})", item.get("submission_spec") or "未说明", item.get("teacher_review") or "未说明"]
            homework.append("| " + " | ".join(str(cell).replace("|", "／").replace("\n", " ") for cell in cells) + " |")
        homework.append("")
    write_note(VAULT / "原课作业.md", {"type": "assignment-index", "status": "全套已检索核读，未逐句听校" if complete_review else "核读中"}, "\n".join(homework))
    report = {"expected_parts": 92, "transcribed_parts": sorted(transcribed), "reviewed_core_lessons": sorted(reviewed),
              "reviewed_supplement_parts": sorted(supplements),
              "explicit_assignment_evidence_count": sum(item.get("kind") == "explicit_assignment" for item in assignments),
              "optional_open_call_evidence_count": sum(item.get("kind") == "optional_open_call" for item in assignments),
              "assignment_count_note": "Evidence records, not unique tasks; one assignment may be restated or developed across lessons.",
              "raw_audio_hours_processed": sum(source_by_part[part]["duration_seconds"] for part in transcribed) / 3600,
              "human_full_listen_claimed": False, "story_capability_claimed": False}
    (HERE / "assembly-status.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    main()
