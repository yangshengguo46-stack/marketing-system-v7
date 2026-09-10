"""Render completed native cleaning responses; never change the original ASR."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
RUN = Path(__file__).resolve().parent
DEST = ROOT / "output/course-learning/charlie-20260906/清洗讲义"
TRANSCRIPTS = ROOT / "research/2026-09-06-charlie-course-organized/transcripts"


def clock(seconds):
    seconds = int(seconds)
    return f"{seconds // 3600:02}:{seconds // 60 % 60:02}:{seconds % 60:02}"


def assemble():
    jobs = json.loads((RUN / "jobs.json").read_text())
    ready, pending = [], []
    for part in [1, *range(61, 93)]:
        part_jobs = [job for job in jobs if job["part"] == part]
        paths = [RUN / (job["key"] + "-clean.json") for job in part_jobs]
        if not all(path.exists() for path in paths):
            pending.append(part)
            continue
        source = TRANSCRIPTS / f"p{part:03}.json"
        data = json.loads(source.read_text())
        source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        segments = {s["index"]: s for s in data["segments"]}
        blocks, flags, uncertainties, threads, reviews = [], [], [], [], []
        for job, path in zip(part_jobs, paths):
            result = json.loads(path.read_text())
            assert source_hash == result["source_json_sha256"] == job["source_json_sha256"]
            assigned = [s for block in result["blocks"] for s in range(block["first_segment"], block["last_segment"] + 1)]
            assert assigned == list(range(job["first_segment"], job["last_segment"] + 1))
            if result["review_length_flag"]:
                flags.append({"job": job["key"], "ratio": result["length_ratio"], "meaning": "Compression review required, not an automatic quality failure or pass."})
            for block in result["blocks"]:
                block = dict(block)
                block.update({"id": f"P{part:03}-B{len(blocks) + 1:03}", "part": part, "disposition": "原生模型全文清洗，未逐句听校", "source_job": job["key"]})
                blocks.append(block)
            uncertainties.extend(result.get("uncertainties", []))
            threads.append(result["thread_id"])
            reviews.append({"job": job["key"], **{key: result[key] for key in ["syntax_repair", "boundary_merges", "lead_review", "lead_full_review", "independent_full_review", "metadata_normalization", "unassigned_corrections"] if key in result}})
        assert [s for b in blocks for s in range(b["first_segment"], b["last_segment"] + 1)] == list(segments)
        doc = [f"# 补充P{part:03}｜{data['title']}：连续清洗讲义", "", "**依据全部机器稿分段清洗，未逐句听校，非讲师逐字原话。** 本文按原顺序保留讲授、案例、问答和插叙。正文保留讲述口吻或使用第三人称转述，其中第一人称以及行业、影片、历史、人物和推广信息均归老师在授课时的说法，本次没有另行核实。疑点就地标出。", "", f"[返回清洗目录](README.md) · [完整机器底稿](../完整课程/补充-P{part:03}.md) · [原有节点与作业定位](../补充/P{part:03}.md)", "", f"覆盖原P{part:03}的全部{len(segments)}个识别段；时间沿用该P的ASR边界。语音识别未覆盖的外语对白、画面、板书和表演不能靠文本清洗补齐。", ""]
        for b in blocks:
            first, last = b["first_segment"], b["last_segment"]
            doc.extend([f"<!-- {b['id']} -->", f"## {b['heading']}", "", f"来源：P{part:03} {clock(segments[first]['source_start_seconds'])}—{clock(segments[last]['source_end_seconds'])}｜S{first}—S{last}", "", b["text"].strip(), ""])
        doc.extend(["## 整理记录与待核项", "", "每段完整输入、原生响应与源段分配均保存。连续覆盖检查只能说明已有机器段全部被分配，不能证明无误识、无语义遗漏或全程视听核验。课中建议、旧作业讲评与招生说明保留其原来身份，不转成全体读者的新作业。", ""])
        if uncertainties:
            doc.extend("- " + str(value).replace("\n", " ") for value in uncertainties)
            doc.append("")
        doc.append(f"[分段与纠错记录](../../../../research/2026-09-08-charlie-course-cleaning/supplement-{part:03}.json)")
        out = DEST / f"补充-P{part:03}.md"
        out.write_text("\n".join(doc) + "\n")
        record = {"lesson": None, "part": part, "source_parts": [{"part": part, "source_json_sha256": source_hash, "segment_count": len(segments)}], "blocks": blocks, "native_threads": threads, "job_reviews": reviews, "length_review_flags": flags, "uncertainties": uncertainties, "output": str(out.relative_to(ROOT)), "output_sha256": hashlib.sha256(out.read_bytes()).hexdigest(), "limits": ["Native full-machine-text cleaning with recorded Lead corrections, not full listening or visual review.", "Lead independent review is confined to recorded scopes; no automatic semantic PASS."]}
        (RUN.parent / f"supplement-{part:03}.json").write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
        ready.append(part)
    print(json.dumps({"assembled_parts": ready, "pending_parts": pending}, ensure_ascii=False))


if __name__ == "__main__":
    assemble()
