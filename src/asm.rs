#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reg {
    Rax,
    Rbx,
    Rdx,
    Rcx,
    Rsi,
    Rdi,
    Rsp,
    Rbp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

// general purpose registers for register allocation
// excludes rbp and rsp (stack management)
// rax (intermediate values, return value)
// r13-r14 (scratch registers)
// r15 (reserved for heap pointer)

/*
    Based on Microsoft x64 calling convention:
        The registers RAX, RCX, RDX, R8, R9, R10, R11 are considered volatile (caller-saved).
        The registers RBX, RBP, RDI, RSI, RSP, R12, R13, R14, and R15 are considered nonvolatile (callee-saved).
    Based on System V AMD64 ABI calling convention (used in Linux):
        The registers RBX, RSP, RBP, and R12–R15 are callee-saved
*/
// This order puts the caller-save registers first
pub static GENERAL_PURPOSE_REGISTERS: [Reg; 10] = [
    Reg::Rdx,
    Reg::Rcx,
    Reg::Rsi,
    Reg::Rdi,
    Reg::R8,
    Reg::R9,
    Reg::R10,
    Reg::R11,
    Reg::Rbx,
    Reg::R12,
];

pub static CALLEE_SAVED_REGISTERS: [Reg; 2] = [Reg::Rbx, Reg::R12];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemRef {
    pub reg: Reg,
    pub offset: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg64 {
    Reg(Reg),
    Signed(i64),
    Unsigned(u64),
    Mem(MemRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg32 {
    Reg(Reg),
    Signed(i32),
    Unsigned(u32),
    Mem(MemRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reg32 {
    Reg(Reg),
    Imm(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovArgs {
    ToReg(Reg, Arg64),
    ToMem(MemRef, Reg32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinArgs {
    ToReg(Reg, Arg32),
    ToMem(MemRef, Reg32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loc {
    Reg(Reg),
    Mem(MemRef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instr {
    Mov(MovArgs),

    Add(BinArgs),
    Sub(BinArgs),
    IMul(BinArgs),
    And(BinArgs),
    Or(BinArgs),
    Xor(BinArgs),
    Shr(BinArgs),
    Sar(BinArgs),
    Cmp(BinArgs),
    Test(BinArgs),

    Push(Arg32),
    Pop(Loc),

    Label(String),

    Call(String),
    Ret,

    Jmp(String),
    Je(String),
    Jne(String),
    Jl(String),
    Jle(String),
    Jg(String),
    Jge(String),

    Js(String),  // jump if msb is 1
    Jz(String),  // jump if result was 0
    Jnz(String), // jump if result was not 0

    Jo(String),  // jump if last arith operation overflowed
    Jno(String), // jump if last arith operation didn't overflow
}

pub fn reg_to_string(r: Reg) -> String {
    match r {
        Reg::Rax => String::from("rax"),
        Reg::Rbx => String::from("rbx"),
        Reg::Rcx => String::from("rcx"),
        Reg::Rdx => String::from("rdx"),
        Reg::Rsi => String::from("rsi"),
        Reg::Rdi => String::from("rdi"),
        Reg::Rsp => String::from("rsp"),
        Reg::Rbp => String::from("rbp"),
        Reg::R8 => String::from("r8"),
        Reg::R9 => String::from("r9"),
        Reg::R10 => String::from("r10"),
        Reg::R11 => String::from("r11"),
        Reg::R12 => String::from("r12"),
        Reg::R13 => String::from("r13"),
        Reg::R14 => String::from("r14"),
        Reg::R15 => String::from("r15"),
    }
}

pub fn imm32_to_string(i: i32) -> String {
    i.to_string()
}

pub fn mem_ref_to_string(m: MemRef) -> String {
    if m.offset == 0 {
        String::from(format!("QWORD [{}]", reg_to_string(m.reg)))
    } else {
        String::from(format!("QWORD [{}{:+}]", reg_to_string(m.reg), m.offset))
    }
}

pub fn reg32_to_string(r_or_i: Reg32) -> String {
    match r_or_i {
        Reg32::Reg(r) => reg_to_string(r),
        Reg32::Imm(i) => imm32_to_string(i),
    }
}

pub fn arg32_to_string(arg: Arg32) -> String {
    match arg {
        Arg32::Reg(r) => reg_to_string(r),
        Arg32::Signed(i) => imm32_to_string(i),
        Arg32::Unsigned(u) => format!("0x{:08x}", u),
        Arg32::Mem(m) => mem_ref_to_string(m),
    }
}

pub fn arg64_to_string(arg: Arg64) -> String {
    match arg {
        Arg64::Reg(r) => reg_to_string(r),
        Arg64::Signed(i) => i.to_string(),
        Arg64::Unsigned(u) => format!("0x{:016x}", u),
        Arg64::Mem(m) => mem_ref_to_string(m),
    }
}

pub fn mov_args_to_string(args: MovArgs) -> String {
    match args {
        MovArgs::ToReg(r, arg) => {
            format!("{}, {}", reg_to_string(r), arg64_to_string(arg))
        }
        MovArgs::ToMem(mem, arg) => {
            format!("{}, {}", mem_ref_to_string(mem), reg32_to_string(arg))
        }
    }
}

pub fn bin_args_to_string(args: BinArgs) -> String {
    match args {
        BinArgs::ToReg(r, arg) => {
            format!("{}, {}", reg_to_string(r), arg32_to_string(arg))
        }
        BinArgs::ToMem(mem, arg) => {
            format!("{}, {}", mem_ref_to_string(mem), reg32_to_string(arg))
        }
    }
}

pub fn loc_to_string(loc: Loc) -> String {
    match loc {
        Loc::Reg(r) => reg_to_string(r),
        Loc::Mem(m) => mem_ref_to_string(m),
    }
}

pub fn instr_to_string(i: &Instr) -> String {
    let indent = " ".repeat(8);
    match i {
        Instr::Mov(args) => format!("{}mov {}", indent, mov_args_to_string(*args)),
        Instr::Add(args) => format!("{}add {}", indent, bin_args_to_string(*args)),
        Instr::Sub(args) => format!("{}sub {}", indent, bin_args_to_string(*args)),
        Instr::IMul(args) => format!("{}imul {}", indent, bin_args_to_string(*args)),
        Instr::And(args) => format!("{}and {}", indent, bin_args_to_string(*args)),
        Instr::Or(args) => format!("{}or {}", indent, bin_args_to_string(*args)),
        Instr::Xor(args) => format!("{}xor {}", indent, bin_args_to_string(*args)),
        Instr::Shr(args) => format!("{}shr {}", indent, bin_args_to_string(*args)),
        Instr::Sar(args) => format!("{}sar {}", indent, bin_args_to_string(*args)),
        Instr::Cmp(args) => format!("{}cmp {}", indent, bin_args_to_string(*args)),
        Instr::Test(args) => format!("{}test {}", indent, bin_args_to_string(*args)),
        Instr::Push(args) => format!("{}push {}", indent, arg32_to_string(*args)),
        Instr::Pop(loc) => format!("{}pop {}", indent, loc_to_string(*loc)),
        Instr::Label(lab) => format!("{}:", lab),
        Instr::Call(func) => format!("{}call {}", indent, func),
        Instr::Ret => format!("{}ret", indent),
        Instr::Jmp(lab) => format!("{}jmp {}", indent, lab),
        Instr::Je(lab) => format!("{}je {}", indent, lab),
        Instr::Jne(lab) => format!("{}jne {}", indent, lab),
        Instr::Jl(lab) => format!("{}jl {}", indent, lab),
        Instr::Jle(lab) => format!("{}jle {}", indent, lab),
        Instr::Jg(lab) => format!("{}jg {}", indent, lab),
        Instr::Jge(lab) => format!("{}jge {}", indent, lab),
        Instr::Js(lab) => format!("{}js {}", indent, lab),
        Instr::Jz(lab) => format!("{}jz {}", indent, lab),
        Instr::Jnz(lab) => format!("{}jnz {}", indent, lab),
        Instr::Jo(lab) => format!("{}jo {}", indent, lab),
        Instr::Jno(lab) => format!("{}jno {}", indent, lab),
    }
}

pub fn instrs_to_string(is: &[Instr]) -> String {
    let mut buf = String::new();
    for i in is {
        buf.push_str(&instr_to_string(&i));
        buf.push_str("\n");
    }
    buf
}
