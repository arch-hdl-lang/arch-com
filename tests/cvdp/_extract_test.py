#!/usr/bin/env python3
"""Extract test harness for interrupt_controller_apb."""
import json, os

JSONL = os.path.expanduser("~/github/cvdp_benchmark/full_dataset/cvdp_v1.0.4_nonagentic_code_generation_no_commercial.jsonl")

with open(JSONL) as f:
    for line in f:
        d = json.loads(line)
        if 'interrupt_controller_0019' in d['id']:
            print("=== ID:", d['id'])
            print("=== HARNESS FILES:")
            for k, v in d['harness']['files'].items():
                print(f"\n--- {k} ---\n{v}")
            break
