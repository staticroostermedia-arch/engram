#!/usr/bin/env python3
"""PEFT/LoRA (QLoRA) SFT on leg_geometry_sft.jsonl — out-of-band train.

Usage:
  .venv-peft/bin/python scripts/peft_leg_geometry_train.py \\
    --dataset data/lora-export/leg_geometry_sft.jsonl \\
    --out data/lora-export/adapters/leg_geometry_lora_v1 \\
    --base-model /home/a/.cache/huggingface/hub/models--google--gemma-4-12B \\
    --max-steps 30 --load-in-4bit

Writes data/lora-export/peft_metrics.json on completion or hard fail.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path


def write_metrics(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dataset", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--base-model", type=str, default="")
    ap.add_argument("--max-steps", type=int, default=30)
    ap.add_argument("--max-length", type=int, default=512)
    ap.add_argument("--load-in-4bit", action="store_true", default=True)
    ap.add_argument("--no-4bit", action="store_true")
    ap.add_argument(
        "--metrics-out",
        type=Path,
        default=Path("data/lora-export/peft_metrics.json"),
    )
    args = ap.parse_args()
    use_4bit = args.load_in_4bit and not args.no_4bit

    if not args.dataset.exists():
        print(f"missing dataset: {args.dataset}", file=sys.stderr)
        return 2
    rows = sum(1 for line in args.dataset.open() if line.strip())
    if rows < 1:
        print("empty dataset", file=sys.stderr)
        return 2

    try:
        import torch
        import peft  # noqa: F401
        import transformers  # noqa: F401
    except ImportError as e:
        write_metrics(
            args.metrics_out,
            {
                "version": 1,
                "status": "blocked_no_torch_env",
                "error": str(e),
                "dataset_rows": rows,
                "dataset": str(args.dataset),
            },
        )
        print(f"PEFT env missing: {e}", file=sys.stderr)
        return 3

    if not args.base_model:
        write_metrics(
            args.metrics_out,
            {
                "version": 1,
                "status": "blocked_no_base_model",
                "dataset_rows": rows,
                "hint": "Pass --base-model (local HF dir or id).",
            },
        )
        print("Set --base-model", file=sys.stderr)
        return 4

    t0 = time.time()
    write_metrics(
        args.metrics_out,
        {
            "version": 1,
            "status": "running",
            "stage": "peft_metrics",
            "dataset_rows": rows,
            "dataset": str(args.dataset),
            "base_model": args.base_model,
            "load_in_4bit": use_4bit,
            "max_steps": args.max_steps,
            "pid": __import__("os").getpid(),
        },
    )

    try:
        from datasets import load_dataset
        from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
        from transformers import (
            AutoModelForCausalLM,
            AutoTokenizer,
            BitsAndBytesConfig,
        )
        from trl import SFTConfig, SFTTrainer
    except ImportError as e:
        write_metrics(
            args.metrics_out,
            {
                "version": 1,
                "status": "blocked_import",
                "error": str(e),
                "dataset_rows": rows,
            },
        )
        print(f"train stack import failed: {e}", file=sys.stderr)
        return 3

    tok = AutoTokenizer.from_pretrained(args.base_model, trust_remote_code=True)
    if tok.pad_token is None:
        tok.pad_token = tok.eos_token
    # Local Gemma-4 snapshot may lack chat_template; TRL SFT requires one if
    # "messages" columns remain. Prefer plain-text field only (see map below).
    if not getattr(tok, "chat_template", None):
        tok.chat_template = (
            "{% for message in messages %}"
            "{{'<|' + message['role'] + '|>\\n' + message['content'] + '\\n'}}"
            "{% endfor %}"
        )

    quant = None
    if use_4bit:
        quant = BitsAndBytesConfig(
            load_in_4bit=True,
            bnb_4bit_quant_type="nf4",
            bnb_4bit_compute_dtype=torch.bfloat16,
            bnb_4bit_use_double_quant=True,
        )

    try:
        model = AutoModelForCausalLM.from_pretrained(
            args.base_model,
            quantization_config=quant,
            device_map="auto",
            trust_remote_code=True,
            torch_dtype=torch.bfloat16 if not use_4bit else None,
        )
    except Exception as e:
        # Gemma4 unified may need AutoModel
        try:
            from transformers import AutoModel

            model = AutoModel.from_pretrained(
                args.base_model,
                quantization_config=quant,
                device_map="auto",
                trust_remote_code=True,
            )
            print(f"fallback AutoModel: {e}", file=sys.stderr)
        except Exception as e2:
            write_metrics(
                args.metrics_out,
                {
                    "version": 1,
                    "status": "failed_model_load",
                    "error": f"{e!r} | fallback {e2!r}",
                    "dataset_rows": rows,
                    "base_model": args.base_model,
                },
            )
            print(f"model load failed: {e2}", file=sys.stderr)
            return 5

    if use_4bit:
        model = prepare_model_for_kbit_training(model)
    if hasattr(model, "gradient_checkpointing_enable"):
        model.gradient_checkpointing_enable()
        if hasattr(model, "config"):
            model.config.use_cache = False

    # Broad target modules — Gemma/LLaMA-style names
    peft_config = LoraConfig(
        r=8,
        lora_alpha=16,
        lora_dropout=0.05,
        bias="none",
        task_type="CAUSAL_LM",
        target_modules=[
            "q_proj",
            "k_proj",
            "v_proj",
            "o_proj",
            "gate_proj",
            "up_proj",
            "down_proj",
        ],
    )
    try:
        model = get_peft_model(model, peft_config)
    except Exception as e:
        write_metrics(
            args.metrics_out,
            {
                "version": 1,
                "status": "failed_peft_attach",
                "error": repr(e),
                "dataset_rows": rows,
            },
        )
        print(f"peft attach failed: {e}", file=sys.stderr)
        return 6

    def to_text(ex: dict) -> dict:
        msgs = ex.get("messages") or []
        parts = []
        for m in msgs:
            role = m.get("role", "")
            content = m.get("content", "")
            parts.append(f"<|{role}|>\n{content}")
        return {"text": "\n".join(parts)}

    ds = load_dataset("json", data_files=str(args.dataset), split="train")
    # Drop messages/meta so TRL does not force apply_chat_template on incomplete tok
    drop = [c for c in ds.column_names if c != "text"]
    ds = ds.map(to_text, remove_columns=drop)

    args.out.mkdir(parents=True, exist_ok=True)
    sft_kwargs = dict(
        output_dir=str(args.out),
        max_steps=args.max_steps,
        per_device_train_batch_size=1,
        gradient_accumulation_steps=8,
        learning_rate=2e-4,
        logging_steps=5,
        save_steps=args.max_steps,
        bf16=True,
        report_to=[],
        gradient_checkpointing=True,
        optim="paged_adamw_8bit",
    )
    # TRL version variance: dataset_text_field / max_seq_length may live on config
    try:
        sft_args = SFTConfig(
            **sft_kwargs,
            dataset_text_field="text",
            max_length=args.max_length,
        )
    except TypeError:
        try:
            sft_args = SFTConfig(
                **sft_kwargs,
                dataset_text_field="text",
                max_seq_length=args.max_length,
            )
        except TypeError:
            sft_args = SFTConfig(**sft_kwargs)

    try:
        trainer = SFTTrainer(
            model=model,
            args=sft_args,
            train_dataset=ds,
            processing_class=tok,
        )
    except TypeError:
        trainer = SFTTrainer(
            model=model,
            args=sft_args,
            train_dataset=ds,
            tokenizer=tok,
            dataset_text_field="text",
            max_seq_length=args.max_length,
        )

    result = trainer.train()
    trainer.save_model(str(args.out))
    tok.save_pretrained(str(args.out))

    loss = None
    if result is not None and getattr(result, "training_loss", None) is not None:
        loss = float(result.training_loss)
    elif result is not None and getattr(result, "metrics", None):
        loss = result.metrics.get("train_loss")

    payload = {
        "version": 1,
        "status": "ok",
        "stage": "peft_metrics",
        "dataset_rows": rows,
        "dataset": str(args.dataset),
        "adapter_path": str(args.out),
        "base_model": args.base_model,
        "max_steps": args.max_steps,
        "load_in_4bit": use_4bit,
        "loss": loss,
        "elapsed_s": round(time.time() - t0, 2),
        "torch": torch.__version__,
        "cuda": torch.cuda.is_available(),
    }
    write_metrics(args.metrics_out, payload)
    print(json.dumps({"status": "ok", "adapter_path": str(args.out), "loss": loss}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
