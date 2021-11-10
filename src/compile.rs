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

fn check_exp<'exp>(
    e: &'exp Exp<Span1>,
    env_arg: &Vec<(&'exp str, ())>,
    mut env_lcl: Vec<(&'exp str, ())>,
    env_fun: &Vec<(&'exp str, usize)>,
) -> Result<(), CompileErr<Span1>> {
    match e {
        Exp::Num(n, ann) => {
            if *n >= i64::MIN >> 1 && *n <= i64::MAX >> 1 {
                Ok(())
            } else {
                Err(CompileErr::Overflow {
                    num: *n,
                    location: *ann,
                })
            }
        }
        Exp::Bool(_, _) => Ok(()),
        Exp::Var(v, span) => {
            match get(&env_arg, v) {
                Some(_) => return Ok(()),
                None => (),
            };
            match get(&env_lcl, v) {
                Some(_) => return Ok(()),
                None => (),
            };
            match get(&env_fun, v) {
                Some(_) => Err(CompileErr::FunctionUsedAsValue {
                    function_name: v.to_string(),
                    location: *span,
                }),
                None => Err(CompileErr::UnboundVariable {
                    unbound: v.to_string(),
                    location: *span,
                }),
            }
        }
        Exp::Prim1(_, e, _) => check_exp(e, env_arg, env_lcl.clone(), env_fun),
        Exp::Prim2(_, e1, e2, _) => {
            check_exp(e1, env_arg, env_lcl.clone(), env_fun)?;
            check_exp(e2, env_arg, env_lcl.clone(), env_fun)
        }
        Exp::Let {
            bindings,
            body,
            ann,
        } => {
            // Check duplicated bindings
            let mut set = HashSet::new();
            for (x, _) in bindings {
                if set.contains(x) {
                    return Err(CompileErr::DuplicateBinding {
                        duplicated_name: x.to_string(),
                        location: *ann,
                    });
                } else {
                    set.insert(x);
                }
            }

            // Process bindings
            for (x, e) in bindings {
                // Evaluate binding's expression and save to RAX
                check_exp(e, env_arg, env_lcl.clone(), env_fun)?;

                // Update environment and save RAX to memory
                env_lcl.push((x, ()));
            }
            check_exp(body, env_arg, env_lcl.clone(), env_fun)
        }
        Exp::If {
            cond,
            thn,
            els,
            ann: _,
        } => {
            check_exp(cond, env_arg, env_lcl.clone(), env_fun)?;
            check_exp(thn, env_arg, env_lcl.clone(), env_fun)?;
            check_exp(els, env_arg, env_lcl.clone(), env_fun)
        }
        Exp::Call(name, exps, ann) => {
            // Check value used as function
            match get(&env_lcl, name) {
                Some(_) => {
                    return Err(CompileErr::ValueUsedAsFunction {
                        variable_name: name.to_string(),
                        location: *ann,
                    })
                }
                None => (),
            };
            match get(&env_arg, name) {
                Some(_) => {
                    return Err(CompileErr::ValueUsedAsFunction {
                        variable_name: name.to_string(),
                        location: *ann,
                    })
                }
                None => (),
            };

            // Check if name in env_fun
            let arity_call = exps.len();
            match get(env_fun, name) {
                Some(arity_def) => {
                    if arity_def != arity_call {
                        return Err(CompileErr::FunctionCalledWrongArity {
                            function_name: name.to_string(),
                            correct_arity: arity_def,
                            arity_used: arity_call,
                            location: *ann,
                        });
                    } else {
                        ()
                    }
                }
                None => {
                    return Err(CompileErr::UndefinedFunction {
                        undefined: name.to_string(),
                        location: *ann,
                    })
                }
            };

            // Recursively call check_exp
            for e in exps {
                check_exp(e, env_arg, env_lcl.clone(), env_fun)?;
            }

            Ok(())
        }
    }
}

fn check_fun<'fun>(
    fun: &'fun FunDecl<Exp<Span1>, Span1>,
    env_fun: &mut Vec<(&'fun str, usize)>,
) -> Result<(), CompileErr<Span1>> {
    let param_num = fun.parameters.len();
    match get(env_fun, &fun.name) {
        Some(_) => {
            return Err(CompileErr::DuplicateFunName {
                duplicated_name: fun.name.clone(),
                location: fun.ann,
            })
        }
        None => {
            env_fun.push((&fun.name, param_num));
        }
    };

    // Check duplicated argument name
    let mut set = HashSet::new();
    for x in &fun.parameters {
        if set.contains(&x) {
            return Err(CompileErr::DuplicateArgName {
                duplicated_name: x.to_string(),
                location: fun.ann,
            });
        } else {
            set.insert(x);
        }
    }
    Ok(())
}

pub fn check_prog(p: &SurfProg<Span1>) -> Result<(), CompileErr<Span1>> {
    let mut env_fun = vec![];

    for fun in &p.funs {
        check_fun(fun, &mut env_fun)?;
    }

    for fun in &p.funs {
        let env_arg: Vec<(&str, ())> = fun.parameters.iter().map(|p| (&p[..], ())).collect();
        let env_lcl = vec![];
        check_exp(&fun.body, &env_arg, env_lcl, &env_fun)?;
    }

    let env_arg = vec![];
    let env_lcl = vec![];
    check_exp(&p.main, &env_arg, env_lcl, &env_fun)
}

pub fn get<T>(env: &[(&str, T)], x: &str) -> Option<T>
where
    T: Copy,
{
    for (y, n) in env.iter().rev() {
        if x == *y {
            return Some(*n);
        }
    }
    None
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
