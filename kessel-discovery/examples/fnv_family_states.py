"""Prefix states for the numbered selector families (last byte inverted), each also with one
separator ('_' or ' ') stepped back, as signed ints for fnv_words (printed comma-joined)."""
import os
P = 0x100000001b3; M = (1 << 64) - 1; PI = pow(P, -1, 1 << 64)


def un(h, c):
    return (((h & M) * PI) & M) ^ c


def s(h):
    h &= M
    return h - (1 << 64) if h >> 63 else h


st = [0xed0cc8938df01b65, 0xed0cc8938df01b15, 0xed0cc8938df01b14] + [0x4d947c79962f2820 + i for i in range(8)]
out = []
for x in st:
    out += [s(x), s(un(x, ord('_'))), s(un(x, ord(' ')))]
print(','.join(map(str, out)))
