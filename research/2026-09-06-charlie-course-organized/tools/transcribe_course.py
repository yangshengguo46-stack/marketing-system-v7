"""Offline streaming ASR; completed parts are durable, resumable checkpoints.

Run with the existing chinese-asr-probe/runtime/.venv/bin/python. No downloads,
network services, source mutations, or temporary full-length WAV files are used.
Interrupted parts restart from their beginning; completed parts are reused only
when their source hash, model settings, and all three final artifacts match.
"""

import argparse
from concurrent.futures import ProcessPoolExecutor, as_completed
from datetime import datetime, timezone
import hashlib
import json
import multiprocessing
from pathlib import Path
import resource
import shutil
import subprocess
import sys
import time
import traceback

ROOT = Path(__file__).resolve().parents[3]
OUTPUT = Path(__file__).resolve().parents[1]
PROBE = ROOT / "research/2026-09-06-course-to-capability/chinese-asr-probe"
MODEL = PROBE / "models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17"
MANIFEST = ROOT / "research/2026-09-06-charlie-course-acquisition/download-verification.json"
SAMPLE_RATE = 16000
ENGINE_ID = "sensevoice-small-int8-2024-07-17-zh-itn-silero-stream-v1"
DISCLAIMER = "离线 ASR 机器转录，未人工听校；可能有错字、漏字或断句错误，不能直接等同于老师逐字原话。"
RECOGNIZER = None
VAD_CONFIG = None
NP = None
SHERPA = None


def utc_now():
    return datetime.now(timezone.utc).isoformat()


def atomic_text(path, text):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(text, encoding="utf-8")
    temporary.replace(path)


def write_json(path, value):
    atomic_text(path, json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def clock_time(seconds, separator="."):
    milliseconds = round(seconds * 1000)
    hours, milliseconds = divmod(milliseconds, 3600000)
    minutes, milliseconds = divmod(milliseconds, 60000)
    seconds, milliseconds = divmod(milliseconds, 1000)
    return f"{hours:02d}:{minutes:02d}:{seconds:02d}{separator}{milliseconds:03d}"


def initialize_worker():
    global NP, SHERPA, RECOGNIZER, VAD_CONFIG
    import numpy
    import sherpa_onnx

    NP, SHERPA = numpy, sherpa_onnx
    RECOGNIZER = SHERPA.OfflineRecognizer.from_sense_voice(
        model=str(MODEL / "model.int8.onnx"), tokens=str(MODEL / "tokens.txt"),
        num_threads=1, provider="cpu", language="zh", use_itn=True,
    )
    VAD_CONFIG = SHERPA.VadModelConfig()
    VAD_CONFIG.silero_vad.model = str(PROBE / "models/silero_vad.onnx")
    VAD_CONFIG.silero_vad.threshold = 0.5
    VAD_CONFIG.silero_vad.min_silence_duration = 0.5
    VAD_CONFIG.silero_vad.min_speech_duration = 0.25
    VAD_CONFIG.silero_vad.max_speech_duration = 20
    VAD_CONFIG.sample_rate = SAMPLE_RATE
    VAD_CONFIG.num_threads = 1
    VAD_CONFIG.provider = "cpu"


def artifacts(part):
    stem = OUTPUT / "transcripts" / f"p{part:03d}"
    return (stem.with_suffix(".json"), stem.with_suffix(".source-time.txt"),
            stem.with_suffix(".source-time.srt"))


def reusable(row):
    paths = artifacts(row["part"])
    if not all(path.is_file() for path in paths):
        return False
    try:
        result = json.loads(paths[0].read_text(encoding="utf-8"))
        return (result["state"] == "completed" and result["engine_id"] == ENGINE_ID
                and result["source"]["sha256"] == row["sha256"]
                and all(hashlib.sha256(path.read_bytes()).hexdigest() == result["artifact_sha256"][path.name]
                        for path in paths[1:]))
    except (ValueError, KeyError, OSError):
        return False


def transcribe(row):
    started = time.perf_counter()
    part = row["part"]
    processing = OUTPUT / "processing"
    status_path = processing / f"p{part:03d}.status.json"
    journal_path = processing / f"p{part:03d}.segments.jsonl"
    error_path = processing / f"p{part:03d}.ffmpeg.log"
    source = {key: row[key] for key in ("part", "cid", "title", "url", "file", "sha256", "bytes", "mtime_ns", "actual_duration_seconds")}
    state = {"part": part, "title": row["title"], "state": "running", "started_at": utc_now(),
             "engine_id": ENGINE_ID, "source": source, "review_status": "machine_asr_not_human_verified",
             "disclaimer": DISCLAIMER, "decoded_audio_seconds": 0, "segment_count": 0}
    write_json(status_path, state)
    process = None
    segments = []
    sample_count = 0
    last_checkpoint = started
    try:
        source_path = ROOT / row["file"]
        stat = source_path.stat()
        if stat.st_size != row["bytes"] or stat.st_mtime_ns != row["mtime_ns"]:
            raise RuntimeError("Source file differs from verified manifest stat; reverify before ASR")
        state["source_integrity"] = "bytes_and_mtime_match_download_verification; sha256_reused_from_verified_manifest"
        vad = SHERPA.VoiceActivityDetector(VAD_CONFIG, buffer_size_in_seconds=60)
        window = VAD_CONFIG.silero_vad.window_size

        def drain(journal):
            while not vad.empty():
                speech = vad.front
                speech_start = speech.start / SAMPLE_RATE
                speech_end = min((speech.start + len(speech.samples)) / SAMPLE_RATE,
                                 sample_count / SAMPLE_RATE)
                stream = RECOGNIZER.create_stream()
                stream.accept_waveform(SAMPLE_RATE, speech.samples)
                decode_started = time.perf_counter()
                RECOGNIZER.decode_stream(stream)
                result = stream.result
                segment = {"index": len(segments) + 1,
                           "source_start_seconds": round(speech_start, 6),
                           "source_end_seconds": round(speech_end, 6),
                           "text": result.text, "tokens": list(result.tokens),
                           "token_timestamps_relative_to_segment_seconds": list(result.timestamps),
                           "decoder_result_raw": str(result),
                           "decode_seconds": round(time.perf_counter() - decode_started, 4)}
                segments.append(segment)
                journal.write(json.dumps(segment, ensure_ascii=False) + "\n")
                journal.flush()
                vad.pop()

        command = [shutil.which("ffmpeg"), "-nostdin", "-hide_banner", "-loglevel", "warning",
                   "-threads", "1", "-i", str(source_path), "-map", "0:a:0", "-vn", "-sn", "-dn",
                   "-ac", "1", "-ar", str(SAMPLE_RATE), "-threads", "1", "-f", "s16le", "pipe:1"]
        with error_path.open("w", encoding="utf-8") as errors, journal_path.open("w", encoding="utf-8") as journal:
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors)
            while True:
                pcm = process.stdout.read(window * 2)
                if not pcm:
                    break
                if len(pcm) % 2:
                    raise RuntimeError("Incomplete final 16-bit PCM sample")
                samples = NP.frombuffer(pcm, dtype="<i2").astype(NP.float32) / 32768
                sample_count += len(samples)
                if len(samples) < window:
                    samples = NP.pad(samples, (0, window - len(samples)))
                vad.accept_waveform(samples)
                drain(journal)
                if time.perf_counter() - last_checkpoint >= 10:
                    state.update(decoded_audio_seconds=round(sample_count / SAMPLE_RATE, 3),
                                 segment_count=len(segments), updated_at=utc_now(),
                                 elapsed_seconds=round(time.perf_counter() - started, 3))
                    write_json(status_path, state)
                    last_checkpoint = time.perf_counter()
            process.stdout.close()
            code = process.wait()
            if code:
                raise RuntimeError(f"ffmpeg exited {code}; see {error_path.name}")
            vad.flush()
            drain(journal)
        decoded_seconds = sample_count / SAMPLE_RATE
        delta = decoded_seconds - row["actual_duration_seconds"]
        if abs(delta) > 1:
            raise RuntimeError(f"Decoded audio duration delta exceeds 1 s: {delta:.6f}")
        elapsed = time.perf_counter() - started
        state.update(state="completed", completed_at=utc_now(), updated_at=utc_now(),
                     decoded_audio_seconds=round(decoded_seconds, 6), segment_count=len(segments),
                     text_characters=sum(len(s["text"]) for s in segments),
                     elapsed_seconds=round(elapsed, 3), rtf=round(elapsed / decoded_seconds, 6),
                     max_rss_bytes_macos=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                     coverage={"input_audio_decoded_from_seconds": 0,
                               "input_audio_decoded_through_seconds": decoded_seconds,
                               "duration_delta_seconds": round(delta, 6),
                               "vad_speech_seconds": round(sum(s["source_end_seconds"] - s["source_start_seconds"] for s in segments), 6),
                               "full_input_processed": True,
                               "silence_and_non_speech_may_be_omitted_by_vad": True,
                               "not_a_human_full_listen_or_visual_review": True},
                     model={"path": str(MODEL / "model.int8.onnx"), "language": "zh", "use_itn": True,
                            "asr_threads": 1, "vad_config": str(VAD_CONFIG), "pcm_sample_rate": SAMPLE_RATE},
                     timestamp_semantics="VAD speech boundaries on original source audio timeline; token positions approximate, not manually aligned.")
        json_path, txt_path, srt_path = artifacts(part)
        header = f"P{part:03d} {row['title']}\n{DISCLAIMER}\n来源：{row['url']}\nCID：{row['cid']}\n源文件 SHA256：{row['sha256']}\n\n"
        atomic_text(txt_path, header + "\n".join(
            f"[{clock_time(s['source_start_seconds'])}–{clock_time(s['source_end_seconds'])}] {s['text']}"
            for s in segments) + "\n")
        atomic_text(srt_path, "\n\n".join(
            f"{s['index']}\n{clock_time(s['source_start_seconds'], ',')} --> {clock_time(s['source_end_seconds'], ',')}\n{s['text']}"
            for s in segments) + "\n")
        state["artifact_sha256"] = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                                    for path in (txt_path, srt_path)}
        write_json(json_path, {**state, "segments": segments})
        write_json(status_path, state)
        return {key: state[key] for key in ("part", "state", "decoded_audio_seconds", "segment_count", "text_characters", "elapsed_seconds", "rtf")}
    except Exception as error:
        if process is not None and process.poll() is None:
            process.terminate()
            process.wait()
        state.update(state="error", updated_at=utc_now(), error=str(error), traceback=traceback.format_exc(),
                     decoded_audio_seconds=sample_count / SAMPLE_RATE, segment_count=len(segments))
        write_json(status_path, state)
        return {"part": part, "state": "error", "error": str(error)}


def part_order():
    order = [2, 3, 4, 7, 8, 40, 41, 5, 6, 9, 10, 1]
    early, late, supplements = list(range(11, 40)), list(range(42, 61)), list(range(61, 93))
    for index in range(max(map(len, (early, late, supplements)))):
        for group in (early, late, supplements):
            if index < len(group):
                order.append(group[index])
    return order


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workers", type=int, default=6)
    parser.add_argument("--parts", help="Comma-separated part numbers; omit for all 92 in course-priority order")
    args = parser.parse_args()
    if not 1 <= args.workers <= 8:
        parser.error("workers must be between 1 and 8")
    if shutil.which("ffmpeg") is None:
        parser.error("ffmpeg is required")
    for directory in (OUTPUT / "transcripts", OUTPUT / "processing"):
        directory.mkdir(parents=True, exist_ok=True)
    rows = {row["part"]: row for row in json.loads(MANIFEST.read_text(encoding="utf-8"))["parts"]}
    selected = list(dict.fromkeys(map(int, args.parts.split(",")))) if args.parts else part_order()
    if any(part not in rows for part in selected):
        parser.error("selected part is absent from verified manifest")
    queue = [rows[part] for part in selected if not reusable(rows[part])]
    summary = {"engine_id": ENGINE_ID, "started_at": utc_now(), "workers": args.workers,
               "selected_parts": selected, "reused_parts": [part for part in selected if reusable(rows[part])],
               "queued_parts": [row["part"] for row in queue], "results": [],
               "resume_policy": "Completed parts reused; interrupted or failed parts restart from beginning.",
               "review_status": "machine_asr_not_human_verified", "disclaimer": DISCLAIMER}
    batch_path = OUTPUT / "processing" / "batch-status.json"
    write_json(batch_path, summary)
    print(json.dumps({"event": "started", "workers": args.workers, "queued": len(queue),
                      "first_parts": summary["queued_parts"][:12], "reused": len(summary["reused_parts"])}, ensure_ascii=False), flush=True)
    started = time.perf_counter()
    if queue:
        with ProcessPoolExecutor(max_workers=args.workers, initializer=initialize_worker,
                                 mp_context=multiprocessing.get_context("spawn")) as executor:
            futures = {executor.submit(transcribe, row): row["part"] for row in queue}
            for future in as_completed(futures):
                try:
                    result = future.result()
                except Exception as error:
                    result = {"part": futures[future], "state": "error", "error": repr(error)}
                summary["results"].append(result)
                summary["updated_at"] = utc_now()
                summary["wall_seconds"] = round(time.perf_counter() - started, 3)
                write_json(batch_path, summary)
                print(json.dumps(result, ensure_ascii=False), flush=True)
    summary["finished_at"] = utc_now()
    summary["wall_seconds"] = round(time.perf_counter() - started, 3)
    summary["complete"] = all(reusable(rows[part]) for part in selected)
    write_json(batch_path, summary)
    print(json.dumps({"event": "finished", "complete": summary["complete"], "wall_seconds": summary["wall_seconds"]}), flush=True)
    return 0 if summary["complete"] else 1


if __name__ == "__main__":
    sys.exit(main())
