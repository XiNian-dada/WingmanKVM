#!/usr/bin/env python3
"""Bound the LL&M integer intermediates with exact affine arithmetic.

Each 1D transform is affine until its final rounding shifts. Keeping affine
coefficients preserves cancellations that naive independent intervals lose.
This checks the mathematical model's bounds, not the complete Rust program.
"""
import json

peak = 0


class Affine:
    def __init__(self, coefficients, box, offset=0):
        global peak
        self.c, self.box, self.offset = coefficients, box, offset
        lo, hi = self.bounds()
        assert -(1 << 31) <= lo <= hi < (1 << 31), (lo, hi)
        peak = max(peak, abs(lo), abs(hi))

    def bounds(self):
        lo = hi = self.offset
        for coefficient, (minimum, maximum) in zip(self.c, self.box):
            lo += coefficient * (minimum if coefficient >= 0 else maximum)
            hi += coefficient * (maximum if coefficient >= 0 else minimum)
        return lo, hi

    def __add__(self, other):
        if isinstance(other, int):
            return Affine(self.c, self.box, self.offset + other)
        return Affine([a + b for a, b in zip(self.c, other.c)], self.box, self.offset + other.offset)

    def __sub__(self, other):
        return self + other * -1

    def __mul__(self, value):
        assert isinstance(value, int)
        return Affine([a * value for a in self.c], self.box, self.offset * value)

    def rounded(self, shift):
        lo, hi = (self + (1 << (shift - 1))).bounds()
        return lo >> shift, hi >> shift


def transform(box, first):
    v = [Affine([int(i == j) for i in range(8)], box) for j in range(8)]
    t0, t7 = v[0] + v[7], v[0] - v[7]
    t1, t6 = v[1] + v[6], v[1] - v[6]
    t2, t5 = v[2] + v[5], v[2] - v[5]
    t3, t4 = v[3] + v[4], v[3] - v[4]
    t10, t13 = t0 + t3, t0 - t3
    t11, t12 = t1 + t2, t1 - t2
    output = [None] * 8
    if first:
        output[0] = ((t10 + t11) * 4).bounds()
        output[4] = ((t10 - t11) * 4).bounds()
    else:
        output[0] = (t10 + t11).rounded(2)
        output[4] = (t10 - t11).rounded(2)
    z1 = (t12 + t13) * 4433
    shift = 11 if first else 15
    output[2] = (z1 + t13 * 6270).rounded(shift)
    output[6] = (z1 + t12 * -15137).rounded(shift)
    z1, z2, z3, z4 = t4 + t7, t5 + t6, t4 + t6, t5 + t7
    z5 = (z3 + z4) * 9633
    a4, a5, a6, a7 = t4 * 2446, t5 * 16819, t6 * 25172, t7 * 12299
    z1, z2 = z1 * -7373, z2 * -20995
    z3, z4 = z3 * -16069 + z5, z4 * -3196 + z5
    output[7] = (a4 + z1 + z3).rounded(shift)
    output[5] = (a5 + z2 + z4).rounded(shift)
    output[3] = (a6 + z2 + z3).rounded(shift)
    output[1] = (a7 + z1 + z4).rounded(shift)
    return output


rows = transform([(-128, 127)] * 8, True)
columns = [transform([row_range] * 8, False) for row_range in rows]
assert all(-32768 <= lo <= hi <= 32767 for column in columns for lo, hi in column)
print(json.dumps({"input": [-128, 127], "row_ranges": rows,
                  "largest_i32_magnitude": peak,
                  "coefficient_min": min(lo for column in columns for lo, _ in column),
                  "coefficient_max": max(hi for column in columns for _, hi in column)}, indent=2))
