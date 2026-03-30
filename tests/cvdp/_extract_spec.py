#!/usr/bin/env python3
import json
JSONL = '/Users/shuqingzhao/github/cvdp_benchmark/full_dataset/cvdp_v1.0.4_nonagentic_code_generation_no_commercial.jsonl'
with open(JSONL) as f:
    for line in f:
        d = json.loads(line)
        if 'interrupt_controller_0019' in d['id']:
            prompt = d['input']['prompt']
            # Find behavior spec
            for keyword in ['current_interrupt', 'servic', 'dispatch', 'priority']:
                idx = prompt.find(keyword)
                if idx >= 0:
                    print(f"--- found '{keyword}' at {idx} ---")
                    print(prompt[max(0,idx-100):idx+500])
                    print()
                    break
            # Print full spec around "Functional" section
            fidx = prompt.find('Functional')
            if fidx >= 0:
                print("=== Functional section ===")
                print(prompt[fidx:fidx+3000])
            break
