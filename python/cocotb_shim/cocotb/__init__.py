"""Cocotb compatibility shim — delegates to arch_cocotb.

This package shadows the real cocotb on PYTHONPATH so cocotb tests and
libraries (e.g. cocotbext-axi) drive the arch sim native model.
"""

from arch_cocotb.decorators import test, start_soon, start, create_task

# Report the cocotb API generation this shim tracks; some libraries
# gate behavior on the major version.
__version__ = "2.0.1+arch"
