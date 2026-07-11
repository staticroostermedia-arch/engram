#!/usr/bin/env python3
"""eval_gate: fixed geometry probes base vs PEFT adapter (QLoRA 4bit).

Writes data/lora-export/eval_gate_metrics.json
Exit 0 if gate pass: >=2 probes score adapter >= base (keyword hit rate) OR
adapter mean score > base mean by epsilon.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from pathlib import Path

PROBES = [
    {
        "id": "crs",
        "prompt": "In Engram geometric memory, what is CRS and the grounded threshold?",
        "keywords": ["0.74", "crs", "confidence", "grounded"],
    },
    {
        "id": "op_bind",
        "prompt": "What does OP_BIND do in Engram VSA / FHRR?",
        "keywords": ["bind", "relation", "op_bind", "vsa", "holographic"],
    },
    {
        "id": "fhrr",
        "prompt": "What is FHRR in Engram?",
        "keywords": ["fhrr", "fourier", "holographic", "phase", "8192"],
    },
    {
        "id": "ritual",
        "prompt": "Name the lean Engram agent wake tool ritual.",
        "keywords": ["session_start", "wake", "ack", "continuation", "lean"],
    },
    {
        "id": "lexicon",
        "prompt": "What is a lexicon:word atom used for in Engram PEFT corpus?",
        "keywords": ["lexicon", "word", "geometry", "training", "mint"],
    },
]


def score_text(text: str, keywords: list[str]) -> float:
    t = text.lower()
    hits = sum(1 for k in keywords if k.lower() in t)
    return hits / max(len(keywords), 1)


def gen(model, tok, prompt: str, max_new: int = 48) -> str:
    import torch

    msgs = f"<|user|>\n{prompt}\n<|assistant|>\n"
    device = next(model.parameters()).device
    inputs = tok(msgs, return_tensors="pt")
    inputs = {k: v.to(device) for k, v in inputs.items()}
    with torch.no_grad():
        out = model.generate(
            **inputs,
            max_new_tokens=max_new,
            do_sample=False,
            pad_token_id=tok.pad_token_id or tok.eos_token_id,
        )
    full = tok.decode(out[0], skip_special_tokens=True)
    if "<|assistant|>" in full:
        return full.split("<|assistant|>")[-1].strip()
    return full[len(msgs) :].strip() if full.startswith(msgs[:20]) else full


def load_model(base: str, adapter: str | None, fourbit: bool):
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig

    tok = AutoTokenizer.from_pretrained(base, trust_remote_code=True)
    if tok.pad_token is None:
        tok.pad_token = tok.eos_token
    if not getattr(tok, "chat_template", None):
        tok.chat_template = (
            "{% for message in messages %}"
            "{{'<|' + message['role'] + '|>\\n' + message['content'] + '\\n'}}"
            "{% endfor %}"
        )
    quant = None
    if fourbit:
        quant = BitsAndBytesConfig(
            load_in_4bit=True,
            bnb_4bit_quant_type="nf4",
            bnb_4bit_compute_dtype=torch.bfloat16,
            bnb_4bit_use_double_quant=True,
        )
    model = AutoModelForCausalLM.from_pretrained(
        base,
        quantization_config=quant,
        device_map="auto",
        trust_remote_code=True,
    )
    if adapter:
        from peft import PeftModel

        model = PeftModel.from_pretrained(model, adapter)
    model.eval()
    return model, tok


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--base-model",
        default="/home/a/.cache/huggingface/hub/models--google--gemma-4-12B",
    )
    ap.add_argument(
        "--adapter",
        default="data/lora-export/adapters/leg_geometry_lora_v1",
    )
    ap.add_argument(
        "--metrics-out",
        default="data/lora-export/eval_gate_metrics.json",
    )
    ap.add_argument("--no-4bit", action="store_true")
    ap.add_argument("--adapter-only", action="store_true", help="Skip base pass (VRAM)")
    args = ap.parse_args()
    fourbit = not args.no_4bit
    t0 = time.time()
    out_path = Path(args.metrics_out)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    results = []
    base_scores = []
    ad_scores = []

    try:
        if not args.adapter_only:
            print("loading base...", flush=True)
            base_m, tok = load_model(args.base_model, None, fourbit)
            for p in PROBES:
                text = gen(base_m, tok, p["prompt"])
                s = score_text(text, p["keywords"])
                base_scores.append(s)
                results.append(
                    {
                        "id": p["id"],
                        "mode": "base",
                        "score": s,
                        "preview": text[:200],
                    }
                )
            del base_m
            import torch

            torch.cuda.empty_cache()

        print("loading adapter...", flush=True)
        ad_m, tok = load_model(args.base_model, args.adapter, fourbit)
        for p in PROBES:
            text = gen(ad_m, tok, p["prompt"])
            s = score_text(text, p["keywords"])
            ad_scores.append(s)
            results.append(
                {
                    "id": p["id"],
                    "mode": "adapter",
                    "score": s,
                    "preview": text[:200],
                }
            )
        del ad_m
    except Exception as e:
        payload = {
            "version": 1,
            "status": "failed",
            "error": repr(e),
            "elapsed_s": round(time.time() - t0, 2),
        }
        out_path.write_text(json.dumps(payload, indent=2) + "\n")
        print(json.dumps(payload))
        return 2

    wins = 0
    per_probe = []
    if base_scores and len(base_scores) == len(ad_scores):
        for i, p in enumerate(PROBES):
            b, a = base_scores[i], ad_scores[i]
            win = a >= b
            if win and a > 0:
                wins += 1
            elif a > b:
                wins += 1
            per_probe.append(
                {"id": p["id"], "base": b, "adapter": a, "adapter_ge_base": a >= b}
            )
        mean_b = sum(base_scores) / len(base_scores)
        mean_a = sum(ad_scores) / len(ad_scores)
        passed = wins >= 2 or mean_a > mean_b + 0.02
    else:
        mean_b = None
        mean_a = sum(ad_scores) / max(len(ad_scores), 1)
        # adapter-only: pass if mean keyword hit >= 0.25
        passed = mean_a >= 0.25
        for i, p in enumerate(PROBES):
            per_probe.append(
                {"id": p["id"], "base": None, "adapter": ad_scores[i], "adapter_ge_base": None}
            )
        wins = sum(1 for s in ad_scores if s >= 0.25)

    payload = {
        "version": 1,
        "status": "ok" if passed else "fail",
        "passed": passed,
        "wins_adapter_ge_base": wins,
        "mean_base": mean_b,
        "mean_adapter": mean_a,
        "per_probe": per_probe,
        "results_preview": results,
        "base_model": args.base_model,
        "adapter": args.adapter,
        "adapter_only": args.adapter_only,
        "elapsed_s": round(time.time() - t0, 2),
        "gate_rule": "wins>=2 or mean_adapter>mean_base+0.02 (or adapter-only mean>=0.25)",
    }
    out_path.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps({k: payload[k] for k in ("status", "passed", "wins_adapter_ge_base", "mean_base", "mean_adapter", "elapsed_s")}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
