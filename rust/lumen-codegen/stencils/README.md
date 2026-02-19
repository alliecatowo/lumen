# Lumen Copy-and-Patch Stencils

This directory contains C source files for the copy-and-patch stencil JIT.
Each `.c` file compiles to a small `.o` object file from which machine code
bytes and relocation entries are extracted to build `stencils.bin`.

## Architecture

Each stencil is a tiny fragment of machine code that implements one LIR opcode.
The stitcher concatenates stencil copies in memory (like copy-and-patch) and
patches in actual register indices, constants, and jump targets.

## Calling Convention

- `r14` = base of the NbValue register file (`NbValue* regs`)
- `r15` = pointer to `VmContext`
- Each `NbValue` is 8 bytes → `regs[n]` is at `[r14 + n*8]`
- Stencils use `rax`, `rcx`, `rdx`, `r8`, `r9`, `r10`, `r11` as scratch
- Stencils fall through to the next stitched stencil (no explicit dispatch jump)

## NaN-Boxing Scheme (NbValue)

```
NAN_MASK      = 0x7FF8_0000_0000_0000
TAG_SHIFT     = 48
PAYLOAD_MASK  = 0x0000_FFFF_FFFF_FFFF

TAG_PTR  = 0   → NAN_MASK | 0 | ptr48      (heap pointer)
TAG_INT  = 1   → NAN_MASK | (1 << 48) | payload48 = 0x7FF9_????_????_????
TAG_BOOL = 3   → NAN_MASK | (3 << 48) | 0/1       = 0x7FFB_0000_0000_000{0,1}
TAG_NULL = 4   → NAN_MASK | (4 << 48) | 0          = 0x7FFC_0000_0000_0000
```

Tag check for INT (top 16 bits == 0x7FF9):
```
shr rdx, 48         ; extract bits 48-63
cmp edx, 0x7FF9     ; compare with TAG_INT marker
```
