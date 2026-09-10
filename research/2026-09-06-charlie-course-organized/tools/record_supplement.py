"""Render an agent-authored, fully read supplemental course card.

This does not summarize or search transcripts. The caller must first read every
segment, author the supplied content, and specify the observed final segment
count. The count guard prevents accidentally publishing a partial reading.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path

from transcribe_course import OUTPUT, clock_time, write_json, atomic_text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--part", type=int, required=True)
    parser.add_argument("--read-segments", type=int, required=True)
    parser.add_argument("--group", choices=["supplements-p001-p062-067", "supplements-p070-071", "supplements-p073", "supplements-p076", "supplements-p078", "supplements-p080-092"], required=True)
    parser.add_argument("--content", type=Path, required=True)
    args = parser.parse_args()
    raw_path = OUTPUT / "transcripts" / f"p{args.part:03d}.json"
    raw = json.loads(raw_path.read_text(encoding="utf-8"))
    if raw["state"] != "completed" or len(raw["segments"]) != args.read_segments:
        parser.error("Transcript incomplete or supplied fully-read segment count differs")
    content = json.loads(args.content.read_text(encoding="utf-8"))
    for key in ("teaching_nodes", "examples", "assignments", "practice_suggestions", "actual_assignment_result", "connections", "uncertainties"):
        if key not in content:
            parser.error(f"Missing authored content field: {key}")
    for node in content["teaching_nodes"]:
        if not 0 <= node["start_seconds"] < node["end_seconds"] <= raw["decoded_audio_seconds"] + 1:
            parser.error("Teaching node outside source duration")
        node["part"] = args.part
    for assignment in content["assignments"]:
        if assignment["kind"] not in {"explicit_assignment", "classroom_exercise", "prior_assignment_recap"}:
            parser.error("Unknown assignment kind")
        for key in ("assignment_form", "submission_spec", "teacher_review", "uncertainty"):
            if key not in assignment:
                parser.error(f"Missing authored assignment field: {key}")
        assignment["part"] = args.part
    data = {"part": args.part, "title": raw["title"], "parts": [args.part], "source": raw["source"],
            "full_transcript_read_parts": [args.part], "transcript_source": str(raw_path),
            "transcript_sha256": hashlib.sha256(raw_path.read_bytes()).hexdigest(),
            "read_segment_indexes": {"first": 1, "last": args.read_segments, "total": args.read_segments},
            "reading_method": "按原顺序完整阅读全部ASR段落；不是关键词命中代替全文阅读",
            "reviewed_at": datetime.now(timezone.utc).isoformat(),
            "review_status": "full_machine_transcript_read_not_human_listening",
            "asr_quality": "离线SenseVoice机器稿，未逐句听校；ASR可能漏字、错字或误断句",
            **content,
            "limits": ["本卡全文读的是机器稿，未完整逐句听看原片。", "时间戳用于回查原视频，不是逐字对齐验证。",
                       "老师观点、行业观察、预测与招生材料均归属于课程原话语境，不是整理者背书。"]}
    destination = OUTPUT / "reviews" / args.group
    destination.mkdir(parents=True, exist_ok=True)
    write_json(destination / f"p{args.part:03d}.json", data)
    lines = [f"# P{args.part:03d} · {raw['title']}", "",
             f"原题保留。来源：[原视频]({raw['source']['url']})；CID {raw['source']['cid']}。已按顺序读完 {args.read_segments} 段机器稿，未逐句听校。", "",
             f"原始文件：[时间戳全文]({OUTPUT / 'transcripts' / f'p{args.part:03d}.source-time.txt'})。本卡整理课程所讲，不独立核实行业判断与宣传。", "",
             "## 按授课顺序整理", ""]
    for node in data["teaching_nodes"]:
        source_url = raw["source"]["url"] + f"&t={int(node['start_seconds'])}"
        lines += [f"### {clock_time(node['start_seconds'])}–{clock_time(node['end_seconds'])} · {node['label']}", "",
                  node["summary"] + f" [回到原片]({source_url})", ""]
    lines += ["## 课程中的具体例子", ""]
    for example in data["examples"]:
        detail = " ".join(example[key] for key in ("original_problem", "teacher_change", "teaching_point") if key in example)
        lines.append(f"- **{example['label']}**（{clock_time(example['start_seconds'])}–{clock_time(example['end_seconds'])}）：{detail}")
    if not data["examples"]:
        lines.append("本段没有展开可单列的故事或改稿案例。")
    lines += ["", "## 实际作业与练习", "", data["actual_assignment_result"], ""]
    for item in data["assignments"] + data["practice_suggestions"]:
        lines.append(f"- {item['task']}（{clock_time(item['start_seconds'])}–{clock_time(item['end_seconds'])}）：{item.get('submission_spec', '未给提交规格')}")
    lines += ["", "## 与其他课程的关系", ""] + [f"- {item}" for item in data["connections"]]
    lines += ["", "## 待回源和边界", ""] + [f"- {item}" for item in data["uncertainties"] + data["limits"]]
    atomic_text(destination / f"p{args.part:03d}.md", "\n".join(lines) + "\n")
    print(json.dumps({"part": args.part, "title": raw["title"], "review_path": str(destination / f"p{args.part:03d}.md"),
                      "read_segments": args.read_segments}, ensure_ascii=False))


if __name__ == "__main__":
    main()
