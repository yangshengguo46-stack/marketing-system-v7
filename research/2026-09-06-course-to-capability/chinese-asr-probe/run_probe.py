"""One bounded, offline 90-second SenseVoice/Silero experiment; no batch mode."""

import json
from pathlib import Path
import resource
import time
import wave

STARTED = time.perf_counter()
import numpy as np
import sherpa_onnx

ROOT = Path(__file__).resolve().parent
MODEL = ROOT / "models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17"
OFFSET = 590.0
SAMPLE_RATE = 16000
with wave.open(str(ROOT / "audio/p005-0590-0680.wav"), "rb") as wav:
    assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (16000, 1, 2)
    samples = np.frombuffer(wav.readframes(wav.getnframes()), dtype="<i2").astype(np.float32) / 32768
assert len(samples) / SAMPLE_RATE == 90.0

load_started = time.perf_counter()
recognizer = sherpa_onnx.OfflineRecognizer.from_sense_voice(
    model=str(MODEL / "model.int8.onnx"), tokens=str(MODEL / "tokens.txt"),
    num_threads=1, provider="cpu", language="zh", use_itn=True,
)
vad_config = sherpa_onnx.VadModelConfig()
vad_config.silero_vad.model = str(ROOT / "models/silero_vad.onnx")
vad_config.silero_vad.threshold = 0.5
vad_config.silero_vad.min_silence_duration = 0.5
vad_config.silero_vad.min_speech_duration = 0.25
vad_config.silero_vad.max_speech_duration = 20
vad_config.sample_rate = SAMPLE_RATE
vad_config.num_threads = 1
vad_config.provider = "cpu"
vad = sherpa_onnx.VoiceActivityDetector(vad_config, buffer_size_in_seconds=120)
model_load_seconds = time.perf_counter() - load_started

recognition_started = time.perf_counter()
vad_started = time.perf_counter()
window = vad_config.silero_vad.window_size
for i in range(0, len(samples), window):
    chunk = samples[i:i + window]
    if len(chunk) < window:
        chunk = np.pad(chunk, (0, window - len(chunk)))
    vad.accept_waveform(chunk)
vad.flush()
vad_seconds = time.perf_counter() - vad_started
segments = []
while not vad.empty():
    speech = vad.front
    relative_start = speech.start / SAMPLE_RATE
    duration = min(len(speech.samples) / SAMPLE_RATE, 90.0 - relative_start)
    stream = recognizer.create_stream()
    stream.accept_waveform(SAMPLE_RATE, speech.samples)
    decoding_started = time.perf_counter()
    recognizer.decode_stream(stream)
    result = stream.result
    row = {
        "index": len(segments) + 1,
        "clip_start_seconds": relative_start,
        "clip_end_seconds": relative_start + duration,
        "source_start_seconds": OFFSET + relative_start,
        "source_end_seconds": OFFSET + relative_start + duration,
        "text": result.text,
        "tokens": list(result.tokens),
        "token_timestamps_relative_to_segment_seconds": list(result.timestamps),
        "decoder_result_raw": str(result),
        "decode_seconds": time.perf_counter() - decoding_started,
    }
    segments.append(row)
    print(json.dumps(row, ensure_ascii=False), flush=True)
    vad.pop()

recognition_seconds = time.perf_counter() - recognition_started
record = {
    "audio_seconds": 90.0, "model_load_seconds": model_load_seconds,
    "vad_seconds": vad_seconds, "recognition_seconds": recognition_seconds,
    "rtf_excluding_model_load": recognition_seconds / 90,
    "script_elapsed_seconds": time.perf_counter() - STARTED,
    "max_rss_bytes_macos": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
    "model": str(MODEL / "model.int8.onnx"), "asr_threads": 1,
    "vad_config": str(vad_config), "language": "zh", "use_itn": True,
    "timestamp_semantics": "VAD speech boundaries; CTC token positions are approximate, not manually aligned. Source timestamps add 590 seconds.",
    "segments": segments,
}
(ROOT / "output/sensevoice.json").write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n")
(ROOT / "output/sensevoice.txt").write_text("\n".join(f"[{s['source_start_seconds']:.3f}–{s['source_end_seconds']:.3f}] {s['text']}" for s in segments) + "\n")

def srt_time(seconds):
    millis = round(seconds * 1000)
    hours, millis = divmod(millis, 3600000)
    minutes, millis = divmod(millis, 60000)
    secs, millis = divmod(millis, 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{millis:03d}"

(ROOT / "output/sensevoice.source-time.srt").write_text("\n\n".join(
    f"{s['index']}\n{srt_time(s['source_start_seconds'])} --> {srt_time(s['source_end_seconds'])}\n{s['text']}" for s in segments
) + "\n")
print(json.dumps({k: v for k, v in record.items() if k != "segments"}, ensure_ascii=False, indent=2))
