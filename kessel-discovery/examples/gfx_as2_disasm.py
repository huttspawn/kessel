"""Disassemble the AS2 (AVM1) bytecode of every DoAction/DoInitAction block in a GFx movie whose
constant pool contains a given string.

usage: python3 -I gfx_as2_disasm.py <movie.gfx> <constant to look for>

Prints one line per action: offset, opcode name, and decoded operands (constant-pool strings,
pushed values, branch targets, function headers). AVM1 opcodes below 0x80 have no payload;
0x80 and up carry a u16 length. Push (0x96) records are typed: 0 string, 1 float, 2 null,
3 undefined, 4 register, 5 bool, 6 double, 7 int, 8 constant8, 9 constant16.
"""
import struct
import sys

sys.path.insert(0, __file__.rsplit('/', 1)[0])
import gfx_dump  # noqa: E402

NAMES = {
    0x00: 'End', 0x04: 'NextFrame', 0x06: 'Play', 0x07: 'Stop', 0x0A: 'Add', 0x0B: 'Subtract', 0x0C: 'Multiply',
    0x0D: 'Divide', 0x0E: 'Equals', 0x0F: 'Less', 0x10: 'And', 0x11: 'Or', 0x12: 'Not', 0x13: 'StringEquals',
    0x14: 'StringLength', 0x15: 'StringExtract', 0x17: 'Pop', 0x18: 'ToInteger', 0x1C: 'GetVariable',
    0x1D: 'SetVariable', 0x20: 'SetTarget2', 0x21: 'StringAdd', 0x22: 'GetProperty', 0x23: 'SetProperty',
    0x24: 'CloneSprite', 0x25: 'RemoveSprite', 0x26: 'Trace', 0x27: 'StartDrag', 0x28: 'EndDrag', 0x29: 'StringLess',
    0x2A: 'Throw', 0x2B: 'CastOp', 0x2C: 'ImplementsOp', 0x30: 'RandomNumber', 0x31: 'MBStringLength', 0x32: 'CharToAscii',
    0x33: 'AsciiToChar', 0x34: 'GetTime', 0x35: 'MBStringExtract', 0x36: 'MBCharToAscii', 0x37: 'MBAsciiToChar',
    0x3A: 'Delete', 0x3B: 'Delete2', 0x3C: 'DefineLocal', 0x3D: 'CallFunction', 0x3E: 'Return', 0x3F: 'Modulo',
    0x40: 'NewObject', 0x41: 'DefineLocal2', 0x42: 'InitArray', 0x43: 'InitObject', 0x44: 'TypeOf', 0x45: 'TargetPath',
    0x46: 'Enumerate', 0x47: 'Add2', 0x48: 'Less2', 0x49: 'Equals2', 0x4A: 'ToNumber', 0x4B: 'ToString',
    0x4C: 'PushDuplicate', 0x4D: 'StackSwap', 0x4E: 'GetMember', 0x4F: 'SetMember', 0x50: 'Increment',
    0x51: 'Decrement', 0x52: 'CallMethod', 0x53: 'NewMethod', 0x54: 'InstanceOf', 0x55: 'Enumerate2',
    0x60: 'BitAnd', 0x61: 'BitOr', 0x62: 'BitXor', 0x63: 'BitLShift', 0x64: 'BitRShift', 0x65: 'BitURShift',
    0x66: 'StrictEquals', 0x67: 'Greater', 0x68: 'StringGreater', 0x69: 'Extends',
    0x81: 'GotoFrame', 0x83: 'GetURL', 0x87: 'StoreRegister', 0x88: 'ConstantPool', 0x8A: 'WaitForFrame',
    0x8B: 'SetTarget', 0x8C: 'GoToLabel', 0x8D: 'WaitForFrame2', 0x8E: 'DefineFunction2', 0x8F: 'Try',
    0x94: 'With', 0x96: 'Push', 0x99: 'Jump', 0x9A: 'GetURL2', 0x9B: 'DefineFunction', 0x9D: 'If',
    0x9E: 'Call', 0x9F: 'GotoFrame2',
}


def cstr(d, i):
    j = d.index(b'\0', i)
    return d[i:j].decode('latin-1'), j + 1


def disasm(d, start):
    pool = []
    i = start
    out = []
    while i < len(d):
        op = d[i]
        at = i
        if op < 0x80:
            out.append((at, NAMES.get(op, f'op{op:02x}'), ''))
            i += 1
            if op == 0:
                pass
            continue
        (ln,) = struct.unpack_from('<H', d, i + 1)
        p = d[i + 3:i + 3 + ln]
        i += 3 + ln
        name = NAMES.get(op, f'op{op:02x}')
        arg = ''
        if op == 0x88:
            (cnt,) = struct.unpack_from('<H', p, 0)
            pool = [w.decode('latin-1') for w in p[2:].split(b'\0')[:cnt]]
            arg = f'{cnt} strings'
        elif op == 0x96:
            vals = []
            k = 0
            while k < len(p):
                t = p[k]
                k += 1
                if t == 0:
                    s, k = cstr(p, k)
                    vals.append(repr(s))
                elif t == 1:
                    vals.append(str(struct.unpack_from('<f', p, k)[0]))
                    k += 4
                elif t == 2:
                    vals.append('null')
                elif t == 3:
                    vals.append('undefined')
                elif t == 4:
                    vals.append(f'r{p[k]}')
                    k += 1
                elif t == 5:
                    vals.append(str(bool(p[k])))
                    k += 1
                elif t == 6:
                    hi, lo = p[k:k + 4], p[k + 4:k + 8]
                    vals.append(str(struct.unpack('<d', lo + hi)[0]))
                    k += 8
                elif t == 7:
                    vals.append(str(struct.unpack_from('<i', p, k)[0]))
                    k += 4
                elif t == 8:
                    vals.append(repr(pool[p[k]]) if p[k] < len(pool) else f'c{p[k]}')
                    k += 1
                elif t == 9:
                    (c,) = struct.unpack_from('<H', p, k)
                    vals.append(repr(pool[c]) if c < len(pool) else f'c{c}')
                    k += 2
                else:
                    vals.append(f'?type{t}')
                    break
            arg = ', '.join(vals)
        elif op in (0x99, 0x9D):
            (off,) = struct.unpack_from('<h', p, 0)
            arg = f'-> {i + off}'
        elif op == 0x87:
            arg = f'r{p[0]}'
        elif op == 0x8E:
            fname, k = cstr(p, 0)
            nparams, nregs, flags = struct.unpack_from('<HBH', p, k)
            k += 5
            params = []
            for _ in range(nparams):
                reg = p[k]
                pname, k = cstr(p, k + 1)
                params.append(f'r{reg}:{pname}')
            (size,) = struct.unpack_from('<H', p, k)
            arg = f'function {fname!r}({", ".join(params)}) regs={nregs} body={size}'
        elif op == 0x9B:
            fname, k = cstr(p, 0)
            (nparams,) = struct.unpack_from('<H', p, k)
            k += 2
            params = []
            for _ in range(nparams):
                pname, k = cstr(p, k)
                params.append(pname)
            (size,) = struct.unpack_from('<H', p, k)
            arg = f'function {fname!r}({", ".join(params)}) body={size}'
        out.append((at, name, arg))
    return out


def main():
    path, needle = sys.argv[1], sys.argv[2]
    _, body = gfx_dump.read_body(open(path, 'rb').read())
    for code, d in gfx_dump.tags(body):
        if code not in (12, 59) or needle.encode() not in d:
            continue
        start = 2 if code == 59 else 0
        print(f'== block tag {code} size {len(d)}')
        for at, name, arg in disasm(d, start):
            print(f'{at:6d} {name} {arg}')


if __name__ == '__main__':
    main()
