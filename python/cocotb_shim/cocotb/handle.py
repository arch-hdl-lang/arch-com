"""Cocotb handle shim: expose arch handle types under cocotb names."""

from arch_cocotb.dut import ArchDUT
from arch_cocotb.signal import ArchSignal, ArchSignalValue

# Libraries type-check against SimHandleBase; our signal handle is the
# closest equivalent.
SimHandleBase = ArchSignal
