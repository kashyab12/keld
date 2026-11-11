# FP/SIMD state and the HFA calling convention

Sable's CPU carries `v[32]` (128-bit V registers), `fpcr` and `fpsr`
(`modules/sable/src/engine.rs` `Cpu`). Scalar B/H/S/D writes zero the rest of
the register. Arithmetic is a bit-exact software implementation
(`src/fp/soft.rs`); no host floating point is used.

## Supported forms

The subset comes from what Xcode clang (`arm64-apple-ios`, `-O0` and `-Os`) emits for
`fixtures/fp/hfa.c` and the compile-only `fixtures/fp/drafts/FirstFrame-main.draft.m`.
To reproduce the evidence, run `scripts/fp-evidence.sh`.

- Loads/stores of B/H/S/D/Q: unsigned offset, unscaled, pre/post-index and
  register offset (`LDR Dt, [Xn, Xm, LSL #3]`). LDP/STP of S/D/Q with
  offset/pre/post addressing. LDR S/D/Q (literal).
- FMOV (register, immediate, W<->S, X<->D including WZR/XZR); `MOV Vd.8B/16B`
  (ORR with Rn == Rm); MOVI byte-replicate (`.8B/.16B`) and byte-mask (`Dd`, `.2D`).
- S and D: FADD, FSUB, FMUL, FDIV, FNMUL, FMADD, FMSUB, FNMADD, FNMSUB, FNEG,
  FABS, FSQRT, FMINNM, FMAXNM, FRINTM, FRINTP, FRINTZ, FCMP/FCMPE (register and
  `#0.0`), FCSEL.
- SCVTF/UCVTF from W/X and the scalar same-width SIMD form (`scvtf d0, d0`, which
  clang emits at `-O0`); FCVTZS/FCVTZU to W/X; FCVT S<->D; SXTL (`SSHLL #0`) to
  4S and 2D (used at `-O0` for `short`/`int` to double).
- MRS/MSR FPCR and FPSR.

Beyond the contract's initial list, the disassembly also shows FNMUL, FMSUB, FNMADD,
FNMSUB, FMINNM/FMAXNM (`fmin`/`fmax`), vector MOV, scalar SCVTF/UCVTF and SXTL,
so these forms are included.

## Semantics and rejection

Only FPCR = 0 is supported: round to nearest even, FZ = 0, DN = 0, AHP = 0 and
no trap enables. Any other `MSR FPCR` value fails with an error, and the CPU is
left unchanged. NaN selection follows the ARM pseudocode (FPProcessNaN,
FPProcessNaNs and FPProcessNaNs3, including the inf×0 with quiet-NaN addend
rule). Tininess is detected before rounding. FPSR accumulates
IOC/DZC/OFC/UFC/IXC exactly. FRINTM/P/Z never set IXC. FNMUL and FMSUB negate
before NaN propagation, as the ARM pseudocode does.

Every other encoding in the FP/SIMD space is rejected with its opcode or
instruction class named. One example is
`unsupported FP/SIMD instruction FMAX 0x1e624820 at 0x1000`.

`scripts/oracle.py` checks each form against Unicorn 2.1.4, comparing all V
registers, FPCR and FPSR. Inputs include quiet and signalling NaNs with
payloads, ±0, ±inf, denormals, and rounding and overflow edges.

## JIT

The x86-64 translator never translates FP/SIMD. Each such instruction is its own
block, and its body is an explicit decline (exit reason 6) that runs exactly that
instruction in the interpreter. `Compiled::declined_fp()` lists the declined
PCs. Tests require identical CPU (including V/FPSR), output and memory under both
dispatch modes.

## ABI

- Host services (SVC syscalls, TLS and ObjC traps) never read or write V, FPCR
  or FPSR.
- `objc_msgSend` and Super2 hand off at guest level and do not touch V.
- When an ownership dealloc continuation returns, it rejects any change to
  d8-d15 (`guest dealloc clobbered callee-saved dN`). It then restores v0-v31
  and FPCR, so the trapping caller sees its own V state. FPSR stays cumulative.
- `objc/signature.rs` walks method encodings. It accepts `B c i q Q f d @ # :`
  plus the exact clang encodings `{CGPoint=dd}`, `{CGSize=dd}`,
  `{CGRect={CGPoint=dd}{CGSize=dd}}` and `{_NSRange=QQ}` (clang never emits
  `{CGRect=dddd}`). It checks clang's offsets: arguments start at 16, and each
  advances by its size rounded up to 4. Arguments must fit x0-x7 and v0-v7.
  Blocks, unions, bitfields, pointers, arrays, qualifiers, other structs and
  HFAs with more than four members reject. Protocol descriptions keep the
  existing `Q`-only profile.

## Integer forms added with this contract

Real clang output for the fixture and the FirstFrame draft also needs integer
forms the original subset lacked: LSRV, MUL (MADD), UDIV, SXTW (SBFM) and LDRSW.
Their whole encoding groups are now supported and checked by the Unicorn oracle:

- UDIV, SDIV (x/0 = 0, INT_MIN/-1 = INT_MIN), LSLV, LSRV, ASRV, RORV.
- MADD, MSUB, SMADDL, SMSUBL, UMADDL, UMSUBL, SMULH, UMULH.
- SBFM in every form (SXTB/SXTH/SXTW, ASR #imm, SBFX, SBFIZ) and EXTR/ROR #imm.
- RBIT, REV16, REV32, REV, CLZ, CLS.
- LDRSB, LDRSH and LDRSW: unsigned offset, unscaled, pre/post-index and
  register offset.

The JIT explicitly declines these forms to the checked Rust transition (exit
reason 4), as it already does for pairs and conditional selects. BFM, CRC32 and
the PAC forms still reject.

## Fixture

`fixtures/fp/hfa.c` (MIT) passes and returns CGRect/CGPoint/CGSize and float
HFAs, nests calls, keeps eight doubles live in d8-d15 across calls and `write`
SVCs, and prints raw IEEE bits for edge arithmetic. Its integer helpers use ordinary
C (variable shifts, 64-bit multiply, `%`, `int` indices), so it also covers the
integer forms above. `scripts/check-fp.sh`
rebuilds it with the host LLVM and compares it byte for byte with
`fixtures/fp/hfa-expected.txt`.
