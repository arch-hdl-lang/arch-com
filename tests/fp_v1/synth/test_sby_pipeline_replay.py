#!/usr/bin/env python3
"""Fast fail-closed checks for the archive-free replay runner (no solver needed)."""
import contextlib
import gzip
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name('crosscheck_sby_pipeline.py')
spec = importlib.util.spec_from_file_location('sby_pipeline', SCRIPT)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class ReplayTests(unittest.TestCase):
    def test_original_32_element_task_fingerprint(self):
        # Original November 23 30-minute run: binds the full concrete RTL and
        # historical output/fill-valid contract, not just a generated snippet.
        meta, comb, rtl = runner.load('e5m2_32_low')
        design = runner.combine(comb, rtl, meta, 'history') + runner.harness(comb, meta, False, 'history')
        self.assertEqual(runner.sha(design.encode()),
                         'b849d155e0d71d4ddab5fa4d4ece1f73bd12aac528aa3d7b4a089af64a970694')
        self.assertEqual(runner.sha(runner.config(meta, 'induct', 1800, 'history').encode()),
                         'bd0769ea1d323582ec63e2f0268065b4437a2ac36ffd90224c0272a16ec25e8b')
        strengthened = runner.combine(comb, rtl, meta) + runner.harness(comb, meta)
        self.assertNotEqual(design, strengthened)

    def test_corrupt_input_is_rejected(self):
        key = 'FMA6_high'
        with tempfile.TemporaryDirectory() as directory:
            dest = Path(directory)
            (dest/(key+'.json')).write_bytes((runner.INPUTS/(key+'.json')).read_bytes())
            (dest/(key+'_comb.sv.gz')).write_bytes(gzip.compress(b'corrupted RTL'))
            with patch.object(runner, 'INPUTS', dest):
                with self.assertRaisesRegex(ValueError, 'integrity mismatch'):
                    runner.load(key)

    def test_missing_mutation_site_is_rejected(self):
        meta, _, rtl = runner.load('FMA6_high')
        changed = runner.mutate(rtl, meta, 'reset-value')
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            runner.mutate(changed, meta, 'reset-value')

    def test_mutation_needs_completed_bounded_counterexample(self):
        meta, comb, rtl = runner.load('FMA6_high')
        for status, code, trace, expected in [
            ('FAIL', 2, True, True), ('FAIL', 2, False, False),
            ('FAIL', 0, True, False), ('UNKNOWN', 4, True, False),
            ('TIMEOUT', 8, False, False), ('PASS', 0, False, False),
        ]:
            with self.subTest(status=status, code=code, trace=trace), tempfile.TemporaryDirectory() as directory:
                dest = Path(directory)/'FMA6_high/skew_startup/check'
                def fake_run(*args, **kwargs):
                    dest.mkdir(parents=True)
                    (dest/'status').write_text(status+'\n')
                    if trace:
                        (dest/'engine_0').mkdir()
                        (dest/'engine_0/trace.vcd').write_text('test witness')
                    import subprocess
                    return subprocess.CompletedProcess(args[0], code)
                with patch.object(runner.subprocess, 'run', side_effect=fake_run), contextlib.redirect_stdout(io.StringIO()):
                    row = runner.run_case(Path(directory), 'FMA6_high', meta, comb, rtl,
                                          'skew', 'startup', 90, 'strengthened')
                self.assertEqual(row['ok'], expected)

    def test_empty_all_cannot_pass_vacuously(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            (base/'inputs').mkdir()
            argv = [str(SCRIPT), '--inputs', str(base/'inputs'), '--all', '--output', str(base/'out')]
            with patch.object(sys, 'argv', argv), patch.object(runner, 'INPUTS', runner.INPUTS), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as raised:
                    runner.main()
                self.assertEqual(raised.exception.code, 2)

    def test_full_generation_matrix_is_well_formed(self):
        # Solver-free coverage of every archived configuration: for each key,
        # the baseline and all six fault injections must assemble a well-formed
        # design in every applicable profile, exercising mutate() (incl. the
        # regex-driven skew/latency/early-valid sites), combine() dedup and
        # observation-port injection, dot_invariants() for each block-dot
        # format, and both harness profiles. Any ValueError from the fixed
        # mutation/observation-site guards is a real regression, caught here
        # without invoking Yosys/SBY. Complements the single-fixture checks
        # above, which only touch FMA6_high + e5m2_32_low.
        keys = sorted(p.stem for p in runner.INPUTS.glob('*.json'))
        self.assertTrue(keys, 'no archived fixtures discovered under INPUTS')
        faults = ['baseline'] + list(runner.FAULTS)
        total = 0
        for key in keys:
            meta, comb, rtl = runner.load(key)
            profiles = ['strengthened', 'history'] if key == 'e5m2_32_low' else ['strengthened']
            for profile in profiles:
                for fault in faults:
                    tasks = ['startup', 'induct', 'window', 'cover'] if fault == 'baseline' else ['startup']
                    for task in tasks:
                        with self.subTest(key=key, profile=profile, fault=fault, task=task):
                            mutated = runner.mutate(rtl, meta, fault)
                            design = (runner.combine(comb, mutated, meta, profile)
                                      + runner.harness(comb, meta, task == 'window', profile))
                            self.assertIn('module top', design)
                            # reference + wrapper core + top at minimum
                            self.assertGreaterEqual(design.count('endmodule'), 3)
                            total += 1
        # 4 keys x (4 baseline tasks + 6 faults) + e5m2_32_low second profile.
        self.assertGreaterEqual(total, 50)


if __name__ == '__main__':
    unittest.main()
