"""Audit completed ASR artifacts and input coverage without claiming listening QA."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys

import transcribe_course as batch


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-all", action="store_true")
    args = parser.parse_args()
    manifest = json.loads(batch.MANIFEST.read_text(encoding="utf-8"))
    completed, pending, failures = [], [], []
    for source in manifest["parts"]:
        part = source["part"]
        json_path, txt_path, srt_path = batch.artifacts(part)
        if not json_path.exists():
            pending.append(part)
            continue
        errors = []
        try:
            data = json.loads(json_path.read_text(encoding="utf-8"))
            segments = data["segments"]
            if not batch.reusable(source):
                errors.append("incomplete, mismatched source/model identity, or changed TXT/SRT artifact")
            if data["part"] != part or data["source"]["cid"] != source["cid"] or data["title"] != source["title"]:
                errors.append("source part/cid/title mismatch")
            audio_seconds = data["decoded_audio_seconds"]
            if abs(audio_seconds - source["actual_duration_seconds"]) > 1:
                errors.append("input duration mismatch over 1 second")
            coverage = data["coverage"]
            if (coverage["input_audio_decoded_from_seconds"] != 0
                    or abs(coverage["input_audio_decoded_through_seconds"] - audio_seconds) > 0.000001
                    or not coverage["full_input_processed"]):
                errors.append("full decode coverage metadata inconsistent")
            if len(segments) != data["segment_count"] or not segments:
                errors.append("segment count mismatch or no detected speech")
            journal_path = batch.OUTPUT / "processing" / f"p{part:03d}.segments.jsonl"
            journal = [json.loads(line) for line in journal_path.read_text(encoding="utf-8").splitlines()]
            if journal != segments:
                errors.append("incremental journal differs from final segments")
            previous_start = -1
            empty_segments = 0
            for index, segment in enumerate(segments, 1):
                start, end = segment["source_start_seconds"], segment["source_end_seconds"]
                if segment["index"] != index or not (0 <= start < end <= audio_seconds + 0.001) or start < previous_start:
                    errors.append(f"invalid source timestamp or index at segment {index}")
                previous_start = start
                empty_segments += not bool(segment["text"].strip())
            if sum(len(segment["text"]) for segment in segments) != data["text_characters"]:
                errors.append("text character count mismatch")
            if batch.DISCLAIMER not in txt_path.read_text(encoding="utf-8"):
                errors.append("TXT missing machine transcript disclaimer")
            expected_srt = "\n\n".join(
                f"{s['index']}\n{batch.clock_time(s['source_start_seconds'], ',')} --> {batch.clock_time(s['source_end_seconds'], ',')}\n{s['text']}"
                for s in segments) + "\n"
            if srt_path.read_text(encoding="utf-8") != expected_srt:
                errors.append("SRT content or timestamps differ from raw JSON")
            expected_txt = "\n".join(
                f"[{batch.clock_time(s['source_start_seconds'])}–{batch.clock_time(s['source_end_seconds'])}] {s['text']}"
                for s in segments) + "\n"
            if not txt_path.read_text(encoding="utf-8").endswith(expected_txt):
                errors.append("TXT content or timestamps differ from raw JSON")
            status = json.loads((batch.OUTPUT / "processing" / f"p{part:03d}.status.json").read_text(encoding="utf-8"))
            if status["state"] != "completed" or status["segment_count"] != len(segments):
                errors.append("processing status differs from final artifact")
            if errors:
                failures.append({"part": part, "errors": errors})
            else:
                completed.append({"part": part, "cid": source["cid"], "title": source["title"],
                                  "source_sha256": source["sha256"], "json_sha256": hashlib.sha256(json_path.read_bytes()).hexdigest(),
                                  "decoded_audio_seconds": audio_seconds, "segment_count": len(segments),
                                  "empty_asr_segments": empty_segments,
                                  "text_characters": data["text_characters"], "rtf": data["rtf"],
                                  "vad_speech_seconds": coverage["vad_speech_seconds"]})
        except (ValueError, KeyError, OSError, TypeError) as error:
            failures.append({"part": part, "errors": [repr(error)]})
    report = {"checked_at": datetime.now(timezone.utc).isoformat(), "expected_parts": len(manifest["parts"]),
              "verified_transcript_parts": len(completed), "pending_parts": pending, "failures": failures,
              "complete": len(completed) == len(manifest["parts"]) and not failures,
              "total_decoded_audio_seconds": round(sum(row["decoded_audio_seconds"] for row in completed), 6),
              "total_segment_count": sum(row["segment_count"] for row in completed),
              "total_text_characters": sum(row["text_characters"] for row in completed),
              "disclaimer": batch.DISCLAIMER,
              "checks": "source identity, model identity, source duration/full decode metadata, incremental journal/final JSON agreement, continuous segment indexes, bounded monotonic source timestamps, JSON/TXT/SRT agreement, artifact hashes, completion state",
              "limits": "No source SHA256 recomputation, human listening, word accuracy scoring, visual slide reading, homework completeness judgment, or factual verification.",
              "parts": completed}
    batch.write_json(batch.OUTPUT / "processing" / "transcript-verification.json", report)
    print(json.dumps({key: value for key, value in report.items() if key != "parts"}, ensure_ascii=False, indent=2))
    return 1 if failures or (args.require_all and pending) else 0


if __name__ == "__main__":
    sys.exit(main())
