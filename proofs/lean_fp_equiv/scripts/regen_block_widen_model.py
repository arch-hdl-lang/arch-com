#!/usr/bin/env python3
"""Generate/check the selected block-widening Lean definitions from current IR.

Also fail closed if the existing FP32 multiply/add Lean model differs from the
current renderer: the exactness theorems use that model, not a fresh duplicate.
"""
import argparse
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
PROJECT = ROOT / 'proofs/lean_fp_equiv'
NAMES = ('arch_e2m1_to_f32', 'arch_e2m3_to_f32', 'arch_e3m2_to_f32',
         'arch_e4m3_to_f32', 'arch_e5m2_to_f32', 'arch_e8m0_to_f32')
HEADER = ('-- Generated from `cargo run --quiet --example dump_fp -- lean`.\n'
          '-- Selected RISC-V widening helpers; no manual arithmetic transcription.\n'
          'namespace ArchFp\n\n')

def definition(text, name):
    match = re.search(r'^def ' + re.escape(name) + r'\b.*?(?=^def |^end ArchFp)',
                      text, re.M | re.S)
    if match is None:
        raise SystemExit(f'missing generated definition: {name}')
    return match.group().rstrip()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    generated = subprocess.run(['cargo', 'run', '--quiet', '--example', 'dump_fp', '--', 'lean'],
                               cwd=ROOT, check=True, capture_output=True, text=True).stdout
    model = (PROJECT / 'ArchFpEquiv/Model.lean').read_text()
    for name in ('arch_f32_mul', 'arch_f32_add'):
        if definition(model, name) != definition(generated, name):
            raise SystemExit(f'existing Lean model differs from current IR: {name}')
    expected = HEADER + '\n\n'.join(definition(generated, n) for n in NAMES) + '\n\nend ArchFp\n'
    target = PROJECT / 'ArchFpEquiv/BlockWidenModel.lean'
    if args.check:
        if target.read_text() != expected:
            raise SystemExit('BlockWidenModel.lean differs from current IR')
        print('Six widening definitions and existing FP32 multiply/add match current IR.')
    else:
        target.write_text(expected)

if __name__ == '__main__':
    main()
