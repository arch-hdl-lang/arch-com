import unittest

from arch_cocotb.result import TestSuccess
from arch_cocotb.runner import _classify_result


class RunnerResultTests(unittest.TestCase):
    def test_test_success_is_a_passing_result(self):
        self.assertEqual(_classify_result(None, TestSuccess("done")), "PASS")
