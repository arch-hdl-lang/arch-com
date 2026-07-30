"""Minimal cocotb.types compatibility: Logic, Range, LogicArray.

The native model is 2-state, so ``X`` and ``Z`` bits convert
deterministically to zero on assignment.
"""


class Logic:
    """A single 4-state logic value; X/Z collapse to 0 on conversion."""

    _CANON = {'0': '0', '1': '1', 'x': 'X', 'z': 'Z',
              'h': '1', 'l': '0', 'u': 'X', 'w': 'X', '-': 'X'}

    def __init__(self, value=0):
        if isinstance(value, Logic):
            self._repr = value._repr
        elif isinstance(value, str):
            c = self._CANON.get(value.lower())
            if c is None:
                raise ValueError(f"invalid Logic literal {value!r}")
            self._repr = c
        elif isinstance(value, (int, bool)):
            if int(value) not in (0, 1):
                raise ValueError(f"invalid Logic integer {value!r}")
            self._repr = '1' if int(value) else '0'
        else:
            raise TypeError(f"cannot construct Logic from {type(value)}")

    def __int__(self):
        return 1 if self._repr == '1' else 0

    def __bool__(self):
        return self._repr == '1'

    def __eq__(self, other):
        if isinstance(other, Logic):
            return self._repr == other._repr
        if isinstance(other, (int, bool)):
            return self._repr in '01' and int(self) == int(other)
        if isinstance(other, str):
            try:
                return self == Logic(other)
            except ValueError:
                return NotImplemented
        return NotImplemented

    def __hash__(self):
        return hash(self._repr)

    def __str__(self):
        return self._repr

    def __repr__(self):
        return f"Logic('{self._repr}')"


class Range:
    """An SV-style index range, e.g. Range(7, 'downto', 0)."""

    def __init__(self, left, direction=None, right=None):
        if direction is None and right is None:
            # Range(n) == n-1 downto 0
            left, direction, right = int(left) - 1, 'downto', 0
        elif right is None:
            left, direction, right = left, 'downto', direction
        self.left = int(left)
        self.right = int(right)
        if isinstance(direction, str):
            d = direction.lower()
            if d not in ('to', 'downto'):
                raise ValueError(f"invalid Range direction {direction!r}")
            self.direction = d
        else:
            raise ValueError("Range direction must be 'to' or 'downto'")

    def __len__(self):
        if self.direction == 'downto':
            return self.left - self.right + 1
        return self.right - self.left + 1

    def __eq__(self, other):
        if isinstance(other, Range):
            return (self.left, self.direction, self.right) == (
                other.left, other.direction, other.right
            )
        return NotImplemented

    def __hash__(self):
        return hash((self.left, self.direction, self.right))

    def __repr__(self):
        return f"Range({self.left}, '{self.direction}', {self.right})"


class LogicArray:
    """A fixed-width array of Logic values.

    Accepts integer, binary-string, iterable-of-Logic, LogicArray, and
    signal-value inputs. Preserves its declared width, supports len(),
    converts to int (X/Z bits read as 0), and equality with integers.
    """

    def __init__(self, value=None, range=None, width=None):
        if isinstance(range, int):
            width = range
            range = None
        if range is not None:
            width = len(range)

        if isinstance(value, LogicArray):
            bits = list(value._bits)
        elif isinstance(value, str):
            bits = [Logic(c) for c in value]
        elif isinstance(value, (int, bool)):
            v = int(value)
            if v < 0:
                if width is None:
                    raise ValueError(
                        "negative LogicArray values require an explicit width"
                    )
                v &= (1 << width) - 1
            w = width if width is not None else max(1, v.bit_length())
            bits = [Logic((v >> i) & 1) for i in reversed(_range_(w))]
        elif value is None:
            if width is None:
                raise ValueError("LogicArray requires a value or a width")
            bits = [Logic(0)] * width
        elif hasattr(value, 'integer') and hasattr(value, '__len__'):
            # ArchSignalValue or similar
            w = width if width is not None else len(value)
            v = int(value)
            bits = [Logic((v >> i) & 1) for i in reversed(_range_(w))]
        else:
            bits = [b if isinstance(b, Logic) else Logic(b) for b in value]

        if width is not None:
            if len(bits) < width:
                bits = [Logic(0)] * (width - len(bits)) + bits
            elif len(bits) > width:
                bits = bits[len(bits) - width:]
        self._bits = bits
        self.range = range if range is not None else Range(
            len(bits) - 1, 'downto', 0
        )

    # ── Conversion ───────────────────────────────────────────────────

    def to_unsigned(self):
        v = 0
        for b in self._bits:
            v = (v << 1) | int(b)  # X/Z read as 0, deterministically
        return v

    def to_signed(self):
        v = self.to_unsigned()
        w = len(self._bits)
        if w > 0 and v >= (1 << (w - 1)):
            v -= 1 << w
        return v

    @property
    def integer(self):
        return self.to_unsigned()

    @property
    def signed_integer(self):
        return self.to_signed()

    @property
    def binstr(self):
        return ''.join(str(b) for b in self._bits)

    def __int__(self):
        return self.to_unsigned()

    def __index__(self):
        return self.to_unsigned()

    def __len__(self):
        return len(self._bits)

    def __iter__(self):
        return iter(self._bits)

    def __getitem__(self, idx):
        return self._bits[idx]

    def __eq__(self, other):
        if isinstance(other, (int, bool)):
            return self.to_unsigned() == int(other)
        if isinstance(other, LogicArray):
            return self.binstr == other.binstr
        if isinstance(other, str):
            return self.binstr == other.upper().replace('_', '')
        if hasattr(other, 'integer'):
            return self.to_unsigned() == other.integer
        return NotImplemented

    def __hash__(self):
        return hash(self.binstr)

    def __repr__(self):
        return f"LogicArray('{self.binstr}')"


def _range_(n):
    return range(n)
