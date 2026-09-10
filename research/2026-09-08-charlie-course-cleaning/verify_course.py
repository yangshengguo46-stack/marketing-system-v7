"""Verify local course artifacts and native request evidence without exposing secrets."""
import datetime
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
RECORDS = Path(__file__).resolve().parent
DEST = ROOT / "output/course-learning/charlie-20260906/清洗讲义"
NATIVE = RECORDS / "native-supplements"


def verify():
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    original = json.loads((DEST.parent / "完整课程/提取核对.json").read_text())
    transcripts = {}
    for source in original["source_parts"]:
        path = ROOT / f"research/2026-09-06-charlie-course-organized/transcripts/p{source['part']:03}.json"
        assert sha(path) == source["source_json_sha256"], path
        transcripts[source["part"]] = json.loads(path.read_text())["segments"]
    for chapter in original["chapter_files"]:
        assert sha(DEST.parent / "完整课程" / chapter["file"]) == chapter["sha256"]
    assert sha(ROOT / original["book"]) == original["book_sha256"]
    chapters = [RECORDS / f"lesson-{n:02}.json" for n in range(1, 35)]
    chapters += [RECORDS / f"supplement-{n:03}.json" for n in [1, *range(61, 93)]]
    checked, pending, source_order, block_count = [], [], [], 0
    for path in chapters:
        if not path.exists():
            pending.append(path.name)
            continue
        record = json.loads(path.read_text())
        output = ROOT / record["output"]
        assert sha(output) == record["output_sha256"], output
        text = output.read_text()
        for source in record["source_parts"]:
            part = source["part"]
            assigned = [i for b in record["blocks"] if b["part"] == part for i in range(b["first_segment"], b["last_segment"] + 1)]
            assert assigned == [s["index"] for s in transcripts[part]], path
            source_order.append(part)
        for block in record["blocks"]:
            comments = re.findall(r'<!--\s*' + re.escape(block["id"]) + r'\s*-->', text)
            anchors = re.findall(r'<a id="' + re.escape(block["id"]) + r'"></a>', text)
            assert len(comments or anchors) == 1, (path, block["id"])
        block_count += len(record["blocks"])
        checked.append(path.name)
    jobs = json.loads((NATIVE / "jobs.json").read_text())
    requests, starts, attempts, failures = {}, {}, [], []
    for path in NATIVE.glob("worker-*-events.ndjson"):
        for line in path.open():
            event = json.loads(line)
            message = event["message"]
            if event["direction"] == "client" and message.get("method") == "turn/start":
                params = message["params"]
                prompt = "\n".join(i["text"] for i in params["input"] if i["type"] == "text")
                requests[params["threadId"]] = prompt
                attempts.append({"thread_id": params["threadId"], "at": event["at"], "log": path.name})
            result = message.get("result", {})
            if "thread" in result and "reasoningEffort" in result:
                starts[result["thread"]["id"]] = {"model": result.get("model"), "effort": result["reasoningEffort"]}
            if message.get("method") == "error":
                params = message.get("params", {})
                failures.append({"thread_id": params.get("threadId"), "at": event["at"], "log": path.name, "message": str(params.get("error", {}))[:800]})
    native_checked, usage, efforts, correction_flags = [], {}, {}, []
    source_to_job = {j["prompt"].split("本段全文：\n", 1)[1]: j["key"] for j in jobs}
    attempts_by_job = {}
    for attempt in attempts:
        source_text = requests[attempt["thread_id"]].split("本段全文：\n", 1)[1]
        key = source_to_job[source_text]
        attempts_by_job.setdefault(key, []).append(attempt["thread_id"])
    for job in jobs:
        path = NATIVE / (job["key"] + "-raw.json")
        if not path.exists():
            continue
        raw = json.loads(path.read_text())
        assert not raw["tool_items"], job["key"]
        cleaned = json.loads((NATIVE / (job["key"] + "-clean.json")).read_text())
        for block in cleaned["blocks"]:
            assert isinstance(block, dict) and block["text"].strip() and block["heading"].strip()
            source_text = "".join(s["text"] for s in transcripts[job["part"]] if block["first_segment"] <= s["index"] <= block["last_segment"])
            for correction in block.get("corrections", []):
                if correction["original"] not in source_text:
                    correction_flags.append({"job": job["key"], "first_segment": block["first_segment"], "last_segment": block["last_segment"], "original_field": correction["original"], "status": "not_an_exact_substring_of_assigned_source_block"})
        assert raw["thread_id"] in requests, job["key"]
        actual = requests[raw["thread_id"]]
        expected = "\n".join(f"S{s['index']:04}: {s['text']}" for s in transcripts[job["part"]] if job["first_segment"] <= s["index"] <= job["last_segment"])
        assert actual.split("本段全文：\n", 1)[1] == expected, job["key"]
        start = starts[raw["thread_id"]]
        efforts[start["effort"]] = efforts.get(start["effort"], 0) + 1
        assert start["model"] == "glm-5-2-260617", job["key"]
        for key, value in (raw.get("usage") or {}).get("total", {}).items():
            if isinstance(value, int):
                usage[key] = usage.get(key, 0) + value
        native_checked.append({"job": job["key"], "thread_id": raw["thread_id"], "actual_prompt_sha256": hashlib.sha256(actual.encode()).hexdigest(), "raw_sha256": sha(path), "source_text_exact": True, **start})
    stories = {"v1": "72e75fa80cf6de17971db88d009d1c87c6c08098e85ec3587c7d8af7965c90f0", "v2": "3d08d5d6060dd2d22cb701ce653d543484f18bcb49b8f638f68a49055f23533c", "v3": "618e9352e307b0a968d1d9931096c9ef6dea91f36038e82edd50e9f9df828585"}
    for version, expected in stories.items():
        assert sha(DEST.parent / f"练习/第一份故事-{version}.md") == expected
    initial = json.loads((RECORDS / "full-course-start.json").read_text())["first_four"]
    for chapter in initial[:3]:
        assert sha(ROOT / chapter["file"]) == chapter["sha256"]
    before_four = RECORDS / "lesson-04-before-navigation.snapshot.txt"
    assert sha(before_four) == initial[3]["sha256"]
    assert (ROOT / initial[3]["file"]).read_text() == before_four.read_text().replace("[第05课机器底稿（待清洗）](../完整课程/正课-05.md)", "[下一课](第05课.md)")
    link_errors = []
    for path in DEST.glob("*.md"):
        text = re.sub(r'```[\s\S]*?```', '', path.read_text())
        for target in re.findall(r'\]\(([^\n)]+)\)', text):
            target = target.strip('<>').split('#', 1)[0]
            if target and not re.match(r'\w+://', target) and not (path.parent / target).exists():
                link_errors.append({"file": path.name, "target": target})
    assert not link_errors, link_errors
    book_verified = False
    manifest_path = RECORDS / "clean-book-manifest.json"
    if not pending and manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
        book = (ROOT / manifest["book"]).read_text()
        assert sha(ROOT / manifest["book"]) == manifest["book_sha256"]
        for index, chapter in enumerate(manifest["chapters"], 1):
            text = (DEST / chapter["file"]).read_text()
            assert sha(DEST / chapter["file"]) == chapter["sha256"]
            section = book.split(f'<a id="chapter-{index:02}"></a>\n\n', 1)[1]
            assert section.startswith(text.rstrip() + '\n')
        assert sorted(source_order) == list(range(1, 93))
        assert len(native_checked) == 163
        book_verified = True
    report = {"at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "scope": "Mechanical artifact and native App Server input verification; not semantic or audiovisual accuracy", "source_hashes_unchanged": 92, "raw_chapters_unchanged": 67, "raw_book_unchanged": True, "story_hashes_unchanged": stories, "checked_chapters": len(checked), "pending_chapters": pending, "blocks_checked": block_count, "book_verified": book_verified, "local_links_valid": True, "native_successful_jobs": len(native_checked), "native_actual_turn_attempts": len(attempts), "native_errors": failures, "efforts": efforts, "successful_jobs_usage_only": usage, "usage_limit": "Native reported successful jobs only; unsuccessful attempts and Codex agents may have additional usage. Not a provider billing total.", "native_jobs": native_checked}
    filename = "full-course-final-checks.json" if book_verified else "full-course-interim-checks.json"
    report["jobs_with_multiple_native_attempts"] = {k: v for k, v in attempts_by_job.items() if len(v) > 1}
    report["correction_original_fields_not_exact_source_substrings"] = len(correction_flags)
    (RECORDS / "correction-source-audit.json").write_text(json.dumps({"scope": "Original fields may include grouped source fragments or previous cleaned text. These flags do not automatically prove an incorrect edit, but the field must not be treated as a verbatim ASR quotation. Use source part and segment IDs to verify.", "flags": correction_flags}, ensure_ascii=False, indent=2) + "\n")
    (RECORDS / filename).write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k not in ["native_jobs", "native_errors", "story_hashes_unchanged"]}, ensure_ascii=False))


if __name__ == "__main__":
    verify()
