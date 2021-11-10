use crate::asm::{instrs_to_string, GENERAL_PURPOSE_REGISTERS};
use crate::asm::{Arg32, Arg64, BinArgs, Instr, Loc, MemRef, MovArgs, Reg, Reg32};
use crate::lexer::Span1;
use crate::syntax::{
    add_tag_sprog, tag_prog, tag_sprog, uniquify_names, Exp, FunDecl, ImmExp, Prim1, Prim2, Prog,
    SeqExp, SeqProg, SurfFunDecl, SurfProg,
};

use crate::graph::Graph;

use std::collections::{HashMap, HashSet};
use std::convert::TryInto;

#[derive(Debug, PartialEq, Eq)]
pub enum CompileErr<Span> {
    UnboundVariable {
        unbound: String,
        location: Span,
    },
    UndefinedFunction {
        undefined: String,
        location: Span,
    },
    // The Span here is the Span of the let-expression that has the two duplicated bindings
    DuplicateBinding {
        duplicated_name: String,
        location: Span,
    },

    Overflow {
        num: i64,
        location: Span,
    },

    DuplicateFunName {
        duplicated_name: String,
        location: Span, // the location of the 2nd function
    },

    DuplicateArgName {
        duplicated_name: String,
        location: Span,
    },

    FunctionUsedAsValue {
        function_name: String,
        location: Span,
    },

    ValueUsedAsFunction {
        variable_name: String,
        location: Span,
    },

    FunctionCalledWrongArity {
        function_name: String,
        correct_arity: usize,
        arity_used: usize,
        location: Span, // location of the function *call*
    },
}

/* A location where a local variable is stored */
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub enum VarLocation {
    Reg(Reg),
    Spill(i32), // a (negative) offset to RBP
}

use std::fmt;
use std::fmt::Display;

impl<Span> Display for CompileErr<Span>
where
    Span: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompileErr::UnboundVariable { unbound, location } => {
                write!(f, "Unbound variable {} at {}", unbound, location)
            }
            CompileErr::UndefinedFunction {
                undefined,
                location,
            } => {
                write!(f, "Undefined function {} called at {}", undefined, location)
            }
            CompileErr::DuplicateBinding {
                duplicated_name,
                location,
            } => write!(
                f,
                "Variable {} defined twice in let-expression at {}",
                duplicated_name, location
            ),

            CompileErr::Overflow { num, location } => write!(
                f,
                "Number literal {} doesn't fit into 63-bit integer at {}",
                num, location
            ),

            CompileErr::DuplicateArgName {
                duplicated_name,
                location,
            } => write!(
                f,
                "multiple arguments named \"{}\" at {}",
                duplicated_name, location,
            ),

            CompileErr::DuplicateFunName {
                duplicated_name,
                location,
            } => write!(
                f,
                "multiple defined functions named \"{}\" at {}",
                duplicated_name, location
            ),

            CompileErr::FunctionUsedAsValue {
                function_name,
                location,
            } => write!(
                f,
                "function {} used in non-function call position at {}",
                function_name, location
            ),

            CompileErr::ValueUsedAsFunction {
                variable_name,
                location,
            } => write!(
                f,
                "value variable {} used in function call position at {}",
                variable_name, location
            ),

            CompileErr::FunctionCalledWrongArity {
                function_name,
                correct_arity,
                arity_used,
                location,
            } => write!(
                f,
                "function {} of arity {} called with {} arguments at {}",
                function_name, correct_arity, arity_used, location
            ),
        }
    }
}

impl<Span> CompileErr<Span> {
    pub fn map_span<F, SpanPrime>(&self, f: F) -> CompileErr<SpanPrime>
    where
        F: FnOnce(&Span) -> SpanPrime,
    {
        match self {
            CompileErr::UnboundVariable { unbound, location } => CompileErr::UnboundVariable {
                unbound: unbound.clone(),
                location: f(location),
            },

            CompileErr::UndefinedFunction {
                undefined,
                location,
            } => CompileErr::UndefinedFunction {
                undefined: undefined.clone(),
                location: f(location),
            },

            CompileErr::DuplicateBinding {
                duplicated_name,
                location,
            } => CompileErr::DuplicateBinding {
                duplicated_name: duplicated_name.clone(),
                location: f(location),
            },
            CompileErr::Overflow { num, location } => CompileErr::Overflow {
                num: *num,
                location: f(location),
            },
            CompileErr::DuplicateArgName {
                duplicated_name,
                location,
            } => CompileErr::DuplicateArgName {
                location: f(location),
                duplicated_name: duplicated_name.clone(),
            },

            CompileErr::DuplicateFunName {
                duplicated_name,
                location,
            } => CompileErr::DuplicateFunName {
                duplicated_name: duplicated_name.clone(),
                location: f(location),
            },

            CompileErr::FunctionUsedAsValue {
                function_name,
                location,
            } => CompileErr::FunctionUsedAsValue {
                function_name: function_name.clone(),
                location: f(location),
            },

            CompileErr::ValueUsedAsFunction {
                variable_name,
                location,
            } => CompileErr::ValueUsedAsFunction {
                variable_name: variable_name.clone(),
                location: f(location),
            },

            CompileErr::FunctionCalledWrongArity {
                function_name,
                correct_arity,
                arity_used,
                location,
            } => CompileErr::FunctionCalledWrongArity {
                function_name: function_name.clone(),
                correct_arity: *correct_arity,
                arity_used: *arity_used,
                location: f(location),
            },
        }
    }
}

pub fn check_prog(p: &SurfProg<Span1>) -> Result<(), CompileErr<Span1>> {
    panic!("NYI")
}

fn sequentialize(e: &Exp<u32>) -> SeqExp<()> {
    panic!("NYI")
}

fn seq_prog(p: &SurfProg<u32>) -> SeqProg<()> {
    Prog {
        funs: p
            .funs
            .iter()
            .map(|d| FunDecl {
                name: d.name.clone(),
                parameters: d.parameters.clone(),
                body: sequentialize(&d.body),
                ann: (),
            })
            .collect(),
        main: sequentialize(&p.main),
        ann: (),
    }
}

pub fn liveness<Ann>(
    e: &SeqExp<Ann>,
    params: &HashSet<String>,
    live_out: HashSet<String>,
) -> SeqExp<HashSet<String>> {
    panic!("NYI")
}

fn liveness_p<Ann1, Ann2>(p: &Prog<SeqExp<Ann1>, Ann2>) -> Prog<SeqExp<HashSet<String>>, Ann2>
where
    Ann2: Clone,
{
    Prog {
        main: liveness(&p.main, &HashSet::new(), HashSet::new()),
        funs: p
            .funs
            .iter()
            .map(|d| FunDecl {
                name: d.name.clone(),
                parameters: d.parameters.clone(),
                body: liveness(
                    &d.body,
                    &d.parameters.iter().cloned().collect(),
                    HashSet::new(),
                ),
                ann: d.ann.clone(),
            })
            .collect(),
        ann: p.ann.clone(),
    }
}

pub fn conflicts<Ann>(e: &SeqExp<(HashSet<String>, Ann)>) -> Graph<String> {
    panic!("NYI")
}

pub fn allocate_registers(
    conflicts: Graph<String>,
    registers: &[Reg],
) -> HashMap<String, VarLocation> {
    panic!("NYI")
}

fn compile_fun(name: &str, params: &[String], body: &SeqExp<(HashSet<String>, u32)>) -> Vec<Instr> {
    let conflicted_e = conflicts(body);

    let variable_assignment = allocate_registers(conflicted_e, &GENERAL_PURPOSE_REGISTERS);
    panic!("NYI: compile_fun")
}

fn compile_main(e: &SeqExp<(HashSet<String>, u32)>) -> Vec<Instr> {
    let mut is = vec![
        Instr::Label(String::from("start_here")),
        Instr::Mov(MovArgs::ToMem(
            MemRef {
                reg: Reg::Rdi,
                offset: 0,
            },
            Reg32::Reg(Reg::Rbp),
        )),
    ];

    let conflicted_e = conflicts(e);

    let variable_assignment = allocate_registers(conflicted_e, &GENERAL_PURPOSE_REGISTERS);

    panic!("NYI: compile_main")
}

pub fn compile_to_string(p: &SurfProg<Span1>) -> Result<String, CompileErr<Span1>> {
    let () = check_prog(p)?;
    let seq_p: Prog<SeqExp<(HashSet<String>, u32)>, u32> = add_tag_sprog(&liveness_p(
        &uniquify_names(&tag_sprog(&seq_prog(&tag_prog(p)))),
    ));

    panic!("TODO: compile functions and main")
}
