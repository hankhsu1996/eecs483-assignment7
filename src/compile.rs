use crate::asm::{instrs_to_string, CALLEE_SAVED_REGISTERS, GENERAL_PURPOSE_REGISTERS};
use crate::asm::{Arg32, Arg64, BinArgs, Instr, Loc, MemRef, MovArgs, Reg, Reg32};
use crate::graph::Graph;
use crate::lexer::Span1;
use crate::syntax::{
    add_tag_sprog, tag_prog, tag_sprog, uniquify_names, Exp, FunDecl, ImmExp, Prim1, Prim2, Prog,
    SeqExp, SeqProg, SurfProg,
};
use itertools::Itertools;
use rand::prelude::IteratorRandom;
use std::collections::{HashMap, HashSet};

static BOOL_TAG32: u32 = 0x01;
static BOOL_TAG: u64 = 0x00_00_00_00_00_00_00_01;
static SNAKE_TRU: u64 = 0xFF_FF_FF_FF_FF_FF_FF_FF;
static SNAKE_FLS: u64 = 0x7F_FF_FF_FF_FF_FF_FF_FF;
static BOOL_MASK: u64 = 0x80_00_00_00_00_00_00_00;

const DEBUG_MODE: bool = false;

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
use std::iter::FromIterator;

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
    match e {
        Exp::Num(n, _) => SeqExp::Imm(ImmExp::Num(*n), ()),
        Exp::Bool(b, _) => SeqExp::Imm(ImmExp::Bool(*b), ()),
        Exp::Var(v, _) => SeqExp::Imm(ImmExp::Var(v.to_string()), ()),
        Exp::Prim1(prim1, e1, ann) => match &**e1 {
            Exp::Num(n, _) => SeqExp::Prim1(*prim1, ImmExp::Num(*n), ()),
            Exp::Bool(b, _) => SeqExp::Prim1(*prim1, ImmExp::Bool(*b), ()),
            Exp::Var(v, _) => SeqExp::Prim1(*prim1, ImmExp::Var(v.to_owned()), ()),
            _ => {
                let s_e1 = sequentialize(e1);
                let name = format!("#prim1_{}", ann);
                SeqExp::Let {
                    var: name.clone(),
                    bound_exp: Box::new(s_e1),
                    body: Box::new(SeqExp::Prim1(*prim1, ImmExp::Var(name), ())),
                    ann: (),
                }
            }
        },
        Exp::Prim2(prim2, e1, e2, ann) => match &**e1 {
            Exp::Num(n1, _) => match &**e2 {
                Exp::Num(n2, _) => SeqExp::Prim2(*prim2, ImmExp::Num(*n1), ImmExp::Num(*n2), ()),
                Exp::Bool(b2, _) => SeqExp::Prim2(*prim2, ImmExp::Num(*n1), ImmExp::Bool(*b2), ()),
                Exp::Var(v2, _) => {
                    SeqExp::Prim2(*prim2, ImmExp::Num(*n1), ImmExp::Var(v2.to_owned()), ())
                }
                _ => {
                    let name2 = format!("#prim2_1_{}", ann);
                    let s_e2 = sequentialize(e2);
                    SeqExp::Let {
                        var: name2.clone(),
                        bound_exp: Box::new(s_e2),
                        body: Box::new(SeqExp::Prim2(
                            *prim2,
                            ImmExp::Num(*n1),
                            ImmExp::Var(name2),
                            (),
                        )),
                        ann: (),
                    }
                }
            },
            Exp::Bool(b1, _) => match &**e2 {
                Exp::Num(n2, _) => SeqExp::Prim2(*prim2, ImmExp::Bool(*b1), ImmExp::Num(*n2), ()),
                Exp::Bool(b2, _) => SeqExp::Prim2(*prim2, ImmExp::Bool(*b1), ImmExp::Bool(*b2), ()),
                Exp::Var(v2, _) => {
                    SeqExp::Prim2(*prim2, ImmExp::Bool(*b1), ImmExp::Var(v2.to_owned()), ())
                }
                _ => {
                    let name2 = format!("#prim2_1_{}", ann);
                    let s_e2 = sequentialize(e2);
                    SeqExp::Let {
                        var: name2.clone(),
                        bound_exp: Box::new(s_e2),
                        body: Box::new(SeqExp::Prim2(
                            *prim2,
                            ImmExp::Bool(*b1),
                            ImmExp::Var(name2),
                            (),
                        )),
                        ann: (),
                    }
                }
            },
            Exp::Var(v1, _) => match &**e2 {
                Exp::Num(n2, _) => {
                    SeqExp::Prim2(*prim2, ImmExp::Var(v1.to_owned()), ImmExp::Num(*n2), ())
                }
                Exp::Bool(b2, _) => {
                    SeqExp::Prim2(*prim2, ImmExp::Var(v1.to_owned()), ImmExp::Bool(*b2), ())
                }
                Exp::Var(v2, _) => SeqExp::Prim2(
                    *prim2,
                    ImmExp::Var(v1.to_owned()),
                    ImmExp::Var(v2.to_owned()),
                    (),
                ),
                _ => {
                    let name2 = format!("#prim2_1_{}", ann);
                    let s_e2 = sequentialize(e2);
                    SeqExp::Let {
                        var: name2.clone(),
                        bound_exp: Box::new(s_e2),
                        body: Box::new(SeqExp::Prim2(
                            *prim2,
                            ImmExp::Var(v1.to_owned()),
                            ImmExp::Var(name2),
                            (),
                        )),
                        ann: (),
                    }
                }
            },
            _ => match &**e2 {
                Exp::Num(n2, _) => {
                    let name1 = format!("#prim2_1_{}", ann);
                    let s_e1 = sequentialize(e1);
                    SeqExp::Let {
                        var: name1.clone(),
                        bound_exp: Box::new(s_e1),
                        body: Box::new(SeqExp::Prim2(
                            *prim2,
                            ImmExp::Var(name1),
                            ImmExp::Num(*n2),
                            (),
                        )),
                        ann: (),
                    }
                }
                Exp::Bool(b2, _) => {
                    let name1 = format!("#prim2_1_{}", ann);
                    let s_e1 = sequentialize(e1);
                    SeqExp::Let {
                        var: name1.clone(),
                        bound_exp: Box::new(s_e1),
                        body: Box::new(SeqExp::Prim2(
                            *prim2,
                            ImmExp::Var(name1),
                            ImmExp::Bool(*b2),
                            (),
                        )),
                        ann: (),
                    }
                }
                Exp::Var(v2, _) => {
                    let name1 = format!("#prim2_1_{}", ann);
                    let s_e1 = sequentialize(e1);
                    SeqExp::Let {
                        var: name1.clone(),
                        bound_exp: Box::new(s_e1),
                        body: Box::new(SeqExp::Prim2(
                            *prim2,
                            ImmExp::Var(name1),
                            ImmExp::Var(v2.to_owned()),
                            (),
                        )),
                        ann: (),
                    }
                }
                _ => {
                    let s_e1 = sequentialize(e1);
                    let s_e2 = sequentialize(e2);
                    let name1 = format!("#prim2_1_{}", ann);
                    let name2 = format!("#prim2_2_{}", ann);
                    SeqExp::Let {
                        var: name1.clone(),
                        bound_exp: Box::new(s_e1),
                        body: Box::new(SeqExp::Let {
                            var: name2.clone(),
                            bound_exp: Box::new(s_e2),
                            body: Box::new(SeqExp::Prim2(
                                *prim2,
                                ImmExp::Var(name1),
                                ImmExp::Var(name2),
                                (),
                            )),
                            ann: (),
                        }),
                        ann: (),
                    }
                }
            },
        },
        Exp::Let {
            bindings,
            body,
            ann,
        } => {
            let mut local_bindings = bindings.clone();
            let first_binding = local_bindings.remove(0);
            if local_bindings.len() == 0 {
                SeqExp::Let {
                    var: first_binding.0,
                    bound_exp: Box::new(sequentialize(&first_binding.1)),
                    body: Box::new(sequentialize(body)),
                    ann: (),
                }
            } else {
                let new_exp = Exp::Let {
                    bindings: local_bindings,
                    body: (*body).clone(),
                    ann: *ann,
                };
                SeqExp::Let {
                    var: first_binding.0,
                    bound_exp: Box::new(sequentialize(&first_binding.1)),
                    body: Box::new(sequentialize(&new_exp)),
                    ann: (),
                }
            }
        }
        Exp::If {
            cond,
            thn,
            els,
            ann,
        } => {
            let s_e = sequentialize(cond);
            let name = format!("#if_{}", ann);
            SeqExp::Let {
                var: name.clone(),
                bound_exp: Box::new(s_e),
                body: Box::new(SeqExp::If {
                    cond: ImmExp::Var(name),
                    thn: Box::new(sequentialize(thn)),
                    els: Box::new(sequentialize(els)),
                    ann: (),
                }),
                ann: (),
            }
        }
        Exp::Call(name, args, ann) => {
            let args_nonimm: Vec<(&Exp<u32>, usize)> = args
                .iter()
                .enumerate()
                .map(|(i, e)| (e, i))
                .filter(|(e, _)| match e {
                    Exp::Num(_, _) => false,
                    Exp::Bool(_, _) => false,
                    Exp::Var(_, _) => false,
                    _ => true,
                })
                .collect();
            let args_new: Vec<ImmExp> = args
                .iter()
                .enumerate()
                .map(|(i, e)| match e {
                    Exp::Num(n, _) => ImmExp::Num(*n),
                    Exp::Bool(b, _) => ImmExp::Bool(*b),
                    Exp::Var(v, _) => ImmExp::Var(v.to_owned()),
                    Exp::Prim1(_, _, _) => ImmExp::Var(format!("#call_{}_{}", ann, i)),
                    Exp::Prim2(_, _, _, _) => ImmExp::Var(format!("#call_{}_{}", ann, i)),
                    Exp::Let {
                        bindings: _,
                        body: _,
                        ann: _,
                    } => ImmExp::Var(format!("#call_{}_{}", ann, i)),
                    Exp::If {
                        cond: _,
                        thn: _,
                        els: _,
                        ann: _,
                    } => ImmExp::Var(format!("#call_{}_{}", ann, i)),
                    Exp::Call(_, _, ann) => ImmExp::Var(format!("#call_{}_{}", ann, i)),
                })
                .collect();
            let seq_call = SeqExp::Call(name.to_string(), args_new, ());
            let mut curr = seq_call;
            for (e, i) in args_nonimm.iter().rev() {
                let arg_name = format!("#call_{}_{}", ann, i);
                curr = SeqExp::Let {
                    var: arg_name,
                    bound_exp: Box::new(sequentialize(e)),
                    body: Box::new(curr),
                    ann: (),
                };
            }
            curr
        }
    }
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

fn get_child_ann(s_body: SeqExp<HashSet<String>>) -> HashSet<String> {
    match s_body {
        SeqExp::Let {
            var: _,
            bound_exp,
            body: _,
            ann: _,
        } => get_child_ann(*bound_exp),
        _ => s_body.ann(),
    }
}

pub fn liveness<Ann>(
    e: &SeqExp<Ann>,
    params: &HashSet<String>,
    live_out: HashSet<String>,
) -> SeqExp<HashSet<String>> {
    match e {
        SeqExp::Imm(imm, _) => {
            let mut set = live_out;
            match imm {
                ImmExp::Num(_) => (),
                ImmExp::Bool(_) => (),
                ImmExp::Var(v) => {
                    set.insert(v.clone());
                }
            }
            set = set
                .difference(params)
                .into_iter()
                .map(|ann| ann.clone())
                .collect();
            SeqExp::Imm(imm.clone(), set)
        }
        SeqExp::Prim1(prim1, e1, _) => {
            let mut set = live_out;
            match e1 {
                ImmExp::Num(_) => (),
                ImmExp::Bool(_) => (),
                ImmExp::Var(v) => {
                    set.insert(v.clone());
                }
            };
            set = set
                .difference(params)
                .into_iter()
                .map(|ann| ann.clone())
                .collect();
            SeqExp::Prim1(*prim1, e1.clone(), set)
        }
        SeqExp::Prim2(prim2, e1, e2, _) => {
            let mut set = live_out;
            match e1 {
                ImmExp::Num(_) => (),
                ImmExp::Bool(_) => (),
                ImmExp::Var(v) => {
                    set.insert(v.clone());
                }
            };
            match e2 {
                ImmExp::Num(_) => (),
                ImmExp::Bool(_) => (),
                ImmExp::Var(v) => {
                    set.insert(v.clone());
                }
            };
            set = set
                .difference(params)
                .into_iter()
                .map(|ann| ann.clone())
                .collect();
            SeqExp::Prim2(*prim2, e1.clone(), e2.clone(), set)
        }
        SeqExp::Let {
            var,
            bound_exp,
            body,
            ann: _,
        } => {
            // Get body's set
            let s_body = liveness(body, params, live_out);
            let ann_body = get_child_ann(s_body.clone());

            // Add var to form the Let's set
            let mut ann_let = ann_body.clone();
            ann_let.insert(var.clone());

            // Get new binding
            let mut ann_bind_in = ann_let.clone();
            ann_bind_in.remove(var);
            let s_bind = liveness(bound_exp, params, ann_bind_in);

            // let mut ann_let = ann_bind;
            // ann_let = ann_let
            //     .difference(params)
            //     .into_iter()
            //     .map(|ann| ann.clone())
            //     .collect();
            SeqExp::Let {
                var: var.clone(),
                bound_exp: Box::new(s_bind),
                body: Box::new(s_body),
                ann: ann_let,
            }
        }
        SeqExp::If {
            cond,
            thn,
            els,
            ann: _,
        } => {
            let s_thn = liveness(thn, params, live_out.clone());
            let s_els = liveness(els, params, live_out);
            let ann_thn = get_child_ann(s_thn.clone());
            let ann_els = get_child_ann(s_els.clone());

            // Build the new SeqExp::If
            let mut ann_if: HashSet<String> = ann_thn
                .union(&ann_els)
                .into_iter()
                .map(|ann| ann.clone())
                .collect();
            match cond {
                ImmExp::Num(_) => (),
                ImmExp::Bool(_) => (),
                ImmExp::Var(v) => {
                    ann_if.insert(v.clone());
                }
            };
            ann_if = ann_if
                .difference(params)
                .into_iter()
                .map(|ann| ann.clone())
                .collect();

            SeqExp::If {
                cond: cond.clone(),
                thn: Box::new(s_thn),
                els: Box::new(s_els),
                ann: ann_if,
            }
        }
        SeqExp::Call(name, imms, _) => {
            let mut set = live_out;
            for imm in imms {
                match imm {
                    ImmExp::Num(_) => (),
                    ImmExp::Bool(_) => (),
                    ImmExp::Var(v) => {
                        set.insert(v.clone());
                    }
                }
            }
            set = set
                .difference(params)
                .into_iter()
                .map(|ann| ann.clone())
                .collect();
            SeqExp::Call(name.to_string(), imms.to_vec(), set)
        }
    }
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

fn conflicts_helper<Ann>(
    e: &SeqExp<(HashSet<String>, Ann)>,
    mut equivs: Vec<HashSet<String>>,
    g: &mut Graph<String>,
) {
    match e {
        SeqExp::Imm(_, _) => (),
        SeqExp::Prim1(_, _, _) => (),
        SeqExp::Prim2(_, _, _, _) => (),
        SeqExp::Let {
            var,
            bound_exp,
            body,
            ann,
        } => {
            // Insert vertex
            g.insert_vertex(var.clone());

            // Update equivs HashSet
            match &**bound_exp {
                SeqExp::Imm(imm, _) => match imm {
                    ImmExp::Var(var2) => {
                        // var and var2 are equivalent. add to equivs HashSet
                        let mut found_equiv = false;
                        for equiv in equivs.iter_mut() {
                            if equiv.contains(var2) {
                                assert!(found_equiv == false);
                                equiv.insert(var.to_string());
                                found_equiv = true;
                            }
                        }
                        if found_equiv == false {
                            equivs.push(HashSet::from_iter(vec![var.clone(), var2.clone()]));
                        }
                    }
                    _ => (),
                },
                _ => (),
            }
            // Get combinations of HashSets in equivs
            let mut equiv_combs = Vec::new();
            for equiv in equivs.iter() {
                for comb_vec in equiv.iter().combinations(2) {
                    let comb_set: HashSet<&String> = HashSet::from_iter(comb_vec);
                    equiv_combs.push(comb_set);
                }
            }

            // Get Let's conflict HashSet
            let mut conf_combs = Vec::new();
            let conf_set = ann.0.clone();
            for comb_vec in conf_set.iter().combinations(2) {
                let comb_set: HashSet<&String> = HashSet::from_iter(comb_vec);
                conf_combs.push(comb_set);
            }

            // Add conflict combinations into graph if it's not in equivalent combinations
            for conf_comb in conf_combs {
                if !equiv_combs.contains(&conf_comb) {
                    let conf_vec = Vec::from_iter(conf_comb);
                    assert!(conf_vec.len() == 2);
                    (*g).insert_edge(conf_vec[0].clone(), conf_vec[1].clone());
                }
            }

            // Recursively call conflict_helper
            conflicts_helper(bound_exp, equivs.clone(), g);
            conflicts_helper(body, equivs, g);
        }

        SeqExp::If {
            cond: _,
            thn,
            els,
            ann: _,
        } => {
            conflicts_helper(thn, equivs.clone(), g);
            conflicts_helper(els, equivs, g);
        }
        SeqExp::Call(_, _, _) => (),
    }
}

pub fn conflicts<Ann>(e: &SeqExp<(HashSet<String>, Ann)>) -> Graph<String> {
    let mut g = Graph::new();
    conflicts_helper(e, vec![], &mut g);
    g
}

// TODO: empty graph?
pub fn allocate_registers(
    mut conflicts: Graph<String>,
    registers: &[Reg],
) -> HashMap<String, VarLocation> {
    // Utility
    let mut rng = rand::thread_rng();
    let regs_all: HashSet<Reg> = HashSet::from_iter(registers.to_owned());
    let k = registers.len();

    // Initialize env
    let mut spill_cnt = 0;
    let mut env = HashMap::<String, VarLocation>::new();

    loop {
        let mut stack = Vec::<String>::new();
        let mut g = conflicts.clone();
        loop {
            // If there're nodes with degree < k
            while g
                .vertices()
                .into_iter()
                .map(|v| g.neighbors(&v).unwrap_or(&HashSet::new()).len())
                .any(|l| l < k)
            {
                // Choose such node
                let node = g
                    .vertices()
                    .into_iter()
                    .filter(|v| g.neighbors(&v).unwrap_or(&HashSet::new()).len() < k)
                    .choose(&mut rng)
                    .unwrap();
                // Delete the node from graph
                g.remove_vertex(&node);
                // Push it onto the stack
                stack.push(node);
            }

            // If the graph is non-empty (and all nodes have degree >= k)
            if g.num_vertices() > 0 {
                let all_deg_ge_k = g
                    .vertices()
                    .into_iter()
                    .all(|v| g.neighbors(&v).unwrap_or(&HashSet::new()).len() >= k);
                assert!(all_deg_ge_k);

                // choose a node, push it into the stack, and delete it
                let node = g
                    .vertices()
                    .into_iter()
                    .max_by(|v1, v2| {
                        g.neighbors(&v1)
                            .unwrap_or(&HashSet::new())
                            .len()
                            .cmp(&g.neighbors(&v2).unwrap_or(&HashSet::new()).len())
                    })
                    .unwrap();
                g.remove_vertex(&node);
                stack.push(node);
            } else {
                break;
            }
        }

        let mut env_try = HashMap::<String, VarLocation>::new();
        loop {
            // println!("start coloring");
            if !stack.is_empty() {
                // Get available registers
                let node = stack.last().unwrap();
                let empty_set = HashSet::new();
                let nbrs = conflicts.neighbors(&node).unwrap_or(&empty_set); // 1 4 5
                let regs_conf: HashSet<Reg> = nbrs
                    .into_iter()
                    .filter_map(|nbr| match env_try.get(nbr) {
                        Some(vl) => match vl {
                            VarLocation::Reg(r) => Some(r.clone()),
                            VarLocation::Spill(_) => panic!(),
                        },
                        None => None,
                    })
                    .collect();
                let regs_avail: HashSet<Reg> =
                    regs_all.difference(&regs_conf).map(|r| r.clone()).collect();

                // If there is no free colors, then choose an uncolored node, spill it
                if regs_avail.len() == 0 {
                    let vtx_spill = stack
                        .iter()
                        .max_by(|v1, v2| {
                            conflicts
                                .neighbors(&v1)
                                .unwrap_or(&HashSet::new())
                                .len()
                                .cmp(&conflicts.neighbors(&v2).unwrap_or(&HashSet::new()).len())
                        })
                        .unwrap();
                    conflicts.remove_vertex(vtx_spill);
                    env.insert(vtx_spill.clone(), VarLocation::Spill(spill_cnt));
                    spill_cnt += 1;
                    break;
                }
                // Successfully color the node
                else {
                    let reg_min = regs_avail.into_iter().min().unwrap();
                    env_try.insert(node.to_owned(), VarLocation::Reg(reg_min));
                    stack.pop();
                }
            } else {
                // Successfully color the graph
                env.extend(env_try.to_owned());
                return env;
            }
        }
    }
}

fn compile_fun(name: &str, params: &[String], body: &SeqExp<(HashSet<String>, u32)>) -> Vec<Instr> {
    // Prepare local environment
    let conflicted_e = conflicts(body);
    let env_lcl = allocate_registers(conflicted_e, &GENERAL_PURPOSE_REGISTERS);

    // Prepare argument environment
    // let env_arg = params.iter().enumerate().map(|(i, p)| (p.as_str(), i as i32)).collect();
    let mut env_arg = vec![];
    for (i, p) in params.into_iter().enumerate() {
        env_arg.push((p.as_str(), i as i32));
    }

    // Transform annotation
    let body = body.map_ann(&mut |(_, ann)| *ann);

    // Compile to instructions
    let mut is = vec![Instr::Label(String::from(name))];
    is.append(&mut compile_to_instrs(&env_arg, &env_lcl, &body));
    is
}

fn compile_main(e: &SeqExp<(HashSet<String>, u32)>) -> Vec<Instr> {
    // Prepare local environment
    let conflicted_e = conflicts(e);
    let env_lcl = allocate_registers(conflicted_e, &GENERAL_PURPOSE_REGISTERS);

    // Transform annotation
    let e = e.map_ann(&mut |(_, ann)| *ann);

    // Compile to instructions
    let mut is = vec![Instr::Label(String::from("start_here"))];
    is.append(&mut compile_to_instrs(&Vec::new(), &env_lcl, &e));
    is
}

fn check_isbool(dest: Reg, lab: &str) -> Vec<Instr> {
    let mut is = Vec::<Instr>::new();
    if DEBUG_MODE {
        return is;
    }
    is.push(Instr::Test(BinArgs::ToReg(
        dest,
        Arg32::Unsigned(BOOL_TAG32),
    )));
    is.push(Instr::Jz(lab.to_string()));
    is
}

fn check_isnum(dest: Reg, lab: &str) -> Vec<Instr> {
    let mut is = Vec::<Instr>::new();
    if DEBUG_MODE {
        return is;
    }
    is.push(Instr::Test(BinArgs::ToReg(
        dest,
        Arg32::Unsigned(BOOL_TAG32),
    )));
    is.push(Instr::Jnz(lab.to_string()));
    is
}

fn check_overflow() -> Vec<Instr> {
    let mut is = Vec::<Instr>::new();
    if DEBUG_MODE {
        return is;
    }
    is.push(Instr::Jo("ovfl_error".to_string()));
    is
}

fn compile_imm<'exp>(
    imm: &'exp ImmExp,
    env_arg: &Vec<(&'exp str, i32)>,
    env_lcl: &HashMap<String, VarLocation>,
    dest: Reg,
) -> Vec<Instr> {
    match imm {
        ImmExp::Num(n) => vec![Instr::Mov(MovArgs::ToReg(dest, Arg64::Signed(*n << 1)))],
        &ImmExp::Bool(b) => match b {
            true => vec![Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU)))],
            false => vec![Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS)))],
        },
        ImmExp::Var(v) => {
            match env_lcl.get(v) {
                Some(vl) => match vl {
                    VarLocation::Reg(reg) => {
                        return vec![Instr::Mov(MovArgs::ToReg(dest, Arg64::Reg(*reg)))]
                    }
                    VarLocation::Spill(n) => {
                        return vec![Instr::Mov(MovArgs::ToReg(
                            dest,
                            Arg64::Mem(MemRef {
                                reg: Reg::Rbp,
                                offset: (n + 1) * (-8),
                            }),
                        ))]
                    }
                },
                None => (),
            };
            match get(&env_arg, v) {
                Some(n) => {
                    vec![Instr::Mov(MovArgs::ToReg(
                        dest,
                        Arg64::Mem(MemRef {
                            reg: Reg::Rbp,
                            // TODO: automatically determine the offset based on
                            // how many callee-saved regs are pushed on the stack
                            offset: (n + 5) * 8,
                        }),
                    ))]
                }
                None => panic!("Cannot get variable"),
            }
        }
    }
}

fn compile_caller_saved(
    env_lcl: &HashMap<String, VarLocation>,
    used_regs: &HashSet<Reg>,
) -> (Vec<Instr>, Vec<Instr>) {
    // TODO: not all caller saved register are assigned before the function call
    // Consider using HashSet to perform difference
    let mut is_push = Vec::<Instr>::new();
    let mut is_pop = Vec::<Instr>::new();

    let caller_saved_regs: HashSet<Reg> = env_lcl
        .into_iter()
        .filter_map(|(_, vl)| match vl {
            VarLocation::Reg(r) => Some(r.clone()),
            VarLocation::Spill(_) => None,
        })
        .filter(|r| !CALLEE_SAVED_REGISTERS.contains(r) && used_regs.contains(r))
        .collect();
    let mut caller_saved_regs_vec: Vec<Reg> = Vec::from_iter(caller_saved_regs);
    let regs_odd = caller_saved_regs_vec.len() % 2 == 1;

    for r in caller_saved_regs_vec.clone() {
        is_push.push(Instr::Push(Arg32::Reg(r)));
    }
    if regs_odd {
        is_push.push(Instr::Push(Arg32::Signed(0)));
    }

    if regs_odd {
        is_pop.push(Instr::Add(BinArgs::ToReg(Reg::Rsp, Arg32::Signed(8))));
    }
    caller_saved_regs_vec.reverse();
    for r in caller_saved_regs_vec {
        is_pop.push(Instr::Pop(Loc::Reg(r)));
    }

    (is_push, is_pop)
}

fn compile_callee_saved() -> (Vec<Instr>, Vec<Instr>) {
    // TODO: pass in used variables, no necessary to push/pop R12 and Rbx
    let mut is_push = Vec::<Instr>::new();
    let mut is_pop = Vec::<Instr>::new();
    is_push.push(Instr::Push(Arg32::Reg(Reg::Rbx)));
    is_push.push(Instr::Push(Arg32::Reg(Reg::R12)));
    is_push.push(Instr::Push(Arg32::Reg(Reg::Rbp)));
    is_push.push(Instr::Push(Arg32::Reg(Reg::R15)));
    is_pop.push(Instr::Pop(Loc::Reg(Reg::R15)));
    is_pop.push(Instr::Pop(Loc::Reg(Reg::Rbp)));
    is_pop.push(Instr::Pop(Loc::Reg(Reg::R12)));
    is_pop.push(Instr::Pop(Loc::Reg(Reg::Rbx)));
    (is_push, is_pop)
}

fn compile_with_env<'exp>(
    e: &'exp SeqExp<u32>,
    env_arg: &Vec<(&'exp str, i32)>,
    env_lcl: &HashMap<String, VarLocation>,
    mut used_regs: HashSet<Reg>,
    dest: Reg,
    is_tail: bool,
) -> Vec<Instr> {
    match e {
        SeqExp::Imm(imm, _) => compile_imm(imm, env_arg, env_lcl, dest),
        SeqExp::Prim1(prim1, imm, ann) => {
            let mut is = compile_imm(imm, env_arg, env_lcl, dest);
            match prim1 {
                Prim1::Add1 => {
                    is.append(&mut check_isnum(dest, "arith_error"));
                    is.push(Instr::Add(BinArgs::ToReg(dest, Arg32::Signed(1 << 1))));
                    is.append(&mut check_overflow());
                }
                Prim1::Sub1 => {
                    is.append(&mut check_isnum(dest, "arith_error"));
                    is.push(Instr::Sub(BinArgs::ToReg(dest, Arg32::Signed(1 << 1))));
                    is.append(&mut check_overflow());
                }
                Prim1::Not => {
                    assert!(dest != Reg::R15);
                    is.append(&mut check_isbool(dest, "logic_error"));
                    is.push(Instr::Mov(MovArgs::ToReg(
                        Reg::R15,
                        Arg64::Unsigned(BOOL_MASK),
                    )));
                    is.push(Instr::Xor(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                }
                Prim1::Print => {
                    let (mut is_push, mut is_pop) = compile_caller_saved(env_lcl, &used_regs);
                    is.append(&mut is_push);
                    is.push(Instr::Mov(MovArgs::ToReg(Reg::Rdi, Arg64::Reg(dest))));
                    is.push(Instr::Call("print_snake_val".to_string()));
                    is.append(&mut is_pop);
                }
                Prim1::IsBool => {
                    let isbool_lab = format!("isbool#{}", ann);
                    is.push(Instr::Mov(MovArgs::ToReg(
                        Reg::R15,
                        Arg64::Unsigned(BOOL_TAG),
                    )));
                    is.push(Instr::Test(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Jnz(isbool_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(isbool_lab));
                }
                Prim1::IsNum => {
                    let isnum_lab = format!("isnum#{}", ann);
                    is.push(Instr::Mov(MovArgs::ToReg(
                        Reg::R15,
                        Arg64::Unsigned(BOOL_TAG),
                    )));
                    is.push(Instr::Test(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Jz(isnum_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(isnum_lab));
                }
                Prim1::PrintStack => todo!(),
            }
            is
        }
        SeqExp::Prim2(prim2, imm1, imm2, ann) => {
            assert!(dest != Reg::R15);

            // Compile imm1 with destination register RAX
            let mut is = compile_imm(imm1, env_arg, env_lcl, dest);

            // Compile imm2 with destination register R15
            is.append(&mut compile_imm(imm2, env_arg, env_lcl, Reg::R15));

            match prim2 {
                Prim2::Add => {
                    is.append(&mut check_isnum(dest, "arith_error"));
                    is.append(&mut check_isnum(Reg::R15, "arith_error"));
                    is.push(Instr::Add(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.append(&mut check_overflow());
                }
                Prim2::Sub => {
                    is.append(&mut check_isnum(dest, "arith_error"));
                    is.append(&mut check_isnum(Reg::R15, "arith_error"));
                    is.push(Instr::Sub(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.append(&mut check_overflow());
                }
                Prim2::Mul => {
                    is.append(&mut check_isnum(dest, "arith_error"));
                    is.append(&mut check_isnum(Reg::R15, "arith_error"));
                    is.push(Instr::Sar(BinArgs::ToReg(dest, Arg32::Signed(1))));
                    is.push(Instr::IMul(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.append(&mut check_overflow());
                }
                Prim2::And => {
                    is.append(&mut check_isbool(dest, "logic_error"));
                    is.append(&mut check_isbool(Reg::R15, "logic_error"));
                    is.push(Instr::And(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                }
                Prim2::Or => {
                    is.append(&mut check_isbool(dest, "logic_error"));
                    is.append(&mut check_isbool(Reg::R15, "logic_error"));
                    is.push(Instr::Or(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                }
                Prim2::Lt => {
                    let lt_lab = format!("lt#{}", ann);
                    is.append(&mut check_isnum(dest, "cmp_error"));
                    is.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    is.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Jl(lt_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(lt_lab));
                }
                Prim2::Gt => {
                    let gt_lab = format!("gt#{}", ann);
                    is.append(&mut check_isnum(dest, "cmp_error"));
                    is.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    is.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Jg(gt_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(gt_lab));
                }
                Prim2::Le => {
                    let le_lab = format!("le#{}", ann);
                    is.append(&mut check_isnum(dest, "cmp_error"));
                    is.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    is.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Jle(le_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(le_lab));
                }
                Prim2::Ge => {
                    let ge_lab = format!("ge#{}", ann);
                    is.append(&mut check_isnum(dest, "cmp_error"));
                    is.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    is.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Jge(ge_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(ge_lab));
                }
                Prim2::Eq => {
                    let eq_lab = format!("eq#{}", ann);
                    is.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Je(eq_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(eq_lab));
                }
                Prim2::Neq => {
                    let neq_lab = format!("neq#{}", ann);
                    is.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    is.push(Instr::Jne(neq_lab.clone()));
                    is.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    is.push(Instr::Label(neq_lab));
                }
            };
            is
        }
        SeqExp::Let {
            var,
            bound_exp,
            body,
            ann: _,
        } => {
            let mut is =
                compile_with_env(bound_exp, env_arg, env_lcl, used_regs.clone(), dest, false);

            // Save RAX to stack memory or registers
            match env_lcl.get(var) {
                Some(vl) => match vl {
                    VarLocation::Reg(r) => {
                        is.push(Instr::Mov(MovArgs::ToReg(*r, Arg64::Reg(dest))));
                        used_regs.insert(*r);
                    }
                    VarLocation::Spill(n) => {
                        is.push(Instr::Mov(MovArgs::ToMem(
                            MemRef {
                                reg: Reg::Rbp,
                                offset: (n + 1) * (-8),
                            },
                            Reg32::Reg(dest),
                        )));
                    }
                },
                None => panic!("Cannot get var location from env_lcl"),
            }

            is.append(&mut compile_with_env(
                body,
                env_arg,
                env_lcl,
                used_regs.clone(),
                dest,
                is_tail,
            ));
            is
        }
        SeqExp::If {
            cond,
            thn,
            els,
            ann,
        } => {
            let else_lab = format!("if_false#{}", ann);
            let done_lab = format!("done#{}", ann);

            // Move cond to RAX
            let mut is = compile_imm(cond, env_arg, env_lcl, dest);

            // Check RAX is boolean
            is.append(&mut check_isbool(dest, "cond_error"));

            // Test RAX's MSB
            is.push(Instr::Mov(MovArgs::ToReg(
                Reg::R15,
                Arg64::Unsigned(BOOL_MASK),
            )));
            is.push(Instr::Test(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));

            // Jump to false
            is.push(Instr::Jz(else_lab.clone()));

            // Compile then expression
            is.extend(compile_with_env(
                thn,
                env_arg,
                env_lcl,
                used_regs.clone(),
                dest,
                is_tail,
            ));

            // Jump to done
            is.push(Instr::Jmp(done_lab.clone()));

            // False label
            is.push(Instr::Label(else_lab));

            // Compile else expression
            is.extend(compile_with_env(
                els,
                env_arg,
                env_lcl,
                used_regs.clone(),
                dest,
                is_tail,
            ));

            // Done label
            is.push(Instr::Label(done_lab));

            is
        }
        SeqExp::Call(name, imms, _) => {
            let mut is = Vec::<Instr>::new();

            // Count callee and caller's #args
            let callee_arg_num = imms.len();
            let caller_arg_num = env_arg.len();

            if is_tail && callee_arg_num <= caller_arg_num {
                // TODO: The cyclic dependency elimination is not optimal.
                // http://maxsnew.com/teaching/eecs-483-fa21/lec_tail-calls_rust_notes.html#%28part._.Reusing_arguments%29

                // Check if any of the argument values come from addresses that we’re about to overwrite
                // imms any, not in lcl env
                let cyclic_dep = imms.iter().any(|imm| match imm {
                    ImmExp::Num(_) => false,
                    ImmExp::Bool(_) => false,
                    ImmExp::Var(v) => !env_lcl.contains_key(v),
                });

                if cyclic_dep {
                    // Push all the new argument values onto the stack
                    for imm in imms {
                        is.append(&mut compile_imm(imm, env_arg, env_lcl, dest));
                        is.push(Instr::Push(Arg32::Reg(Reg::Rax)));
                    }

                    // Pop them (in the opposite order) into their correct locations
                    for (i, _) in imms.iter().enumerate().rev() {
                        is.push(Instr::Pop(Loc::Mem(MemRef {
                            reg: Reg::Rbp,
                            // TODO: automatically determine the offset based on
                            // how many callee-saved regs are pushed on the stack
                            offset: (i as i32 + 5) * 8,
                        })))
                    }
                } else {
                    // Overwrite our parameters with the callee's parameters
                    for (i, imm) in imms.iter().enumerate() {
                        is.append(&mut compile_imm(imm, env_arg, env_lcl, dest));
                        is.push(Instr::Mov(MovArgs::ToMem(
                            MemRef {
                                reg: Reg::Rbp,
                                // TODO: automatically determine the offset based on
                                // how many callee-saved regs are pushed on the stack
                                offset: (i as i32 + 5) * 8,
                            },
                            Reg32::Reg(Reg::Rax),
                        )));
                    }
                }

                // Make the stack pointer point at base pointer
                is.push(Instr::Mov(MovArgs::ToReg(Reg::Rsp, Arg64::Reg(Reg::Rbp))));
                // Restore callee-saved registers
                let (_, mut is_pop) = compile_callee_saved();
                is.append(&mut is_pop);
                // Jump to the function
                is.push(Instr::Jmp(name.to_string()));
            } else {
                let (mut is_push, mut is_pop) = compile_caller_saved(env_lcl, &used_regs);
                is.append(&mut is_push);
                let offset = -8 * imms.len() as i32;
                for imm in imms.iter().rev() {
                    is.append(&mut compile_imm(imm, env_arg, env_lcl, dest));
                    is.push(Instr::Push(Arg32::Reg(dest)));
                }
                is.push(Instr::Call(name.to_string()));
                is.push(Instr::Sub(BinArgs::ToReg(Reg::Rsp, Arg32::Signed(offset))));
                is.append(&mut is_pop);
            }
            is
        }
    }
}

fn compile_to_instrs<'exp>(
    env_arg: &Vec<(&'exp str, i32)>,
    env_lcl: &HashMap<String, VarLocation>,
    e: &SeqExp<u32>,
) -> Vec<Instr> {
    let space_lcl = env_lcl
        .values()
        .filter(|v| matches!(v, VarLocation::Spill(_)))
        .count() as i32;
    let space;
    if env_arg.len() % 2 == 0 {
        space = space_lcl / 2 * 2 + 1;
    } else {
        space = (space_lcl + 1) / 2 * 2;
    }

    let mut is = Vec::<Instr>::new();
    let (mut is_push, mut is_pop) = compile_callee_saved();
    is.append(&mut is_push);
    is.push(Instr::Mov(MovArgs::ToReg(Reg::Rbp, Arg64::Reg(Reg::Rsp))));
    if space != 0 {
        is.push(Instr::Sub(BinArgs::ToReg(
            Reg::Rsp,
            Arg32::Signed(space * 8),
        )));
    }
    is.append(&mut compile_with_env(
        e,
        env_arg,
        env_lcl,
        HashSet::<Reg>::new(),
        Reg::Rax,
        true,
    ));
    is.push(Instr::Mov(MovArgs::ToReg(Reg::Rsp, Arg64::Reg(Reg::Rbp))));
    is.append(&mut is_pop);
    is.push(Instr::Ret);
    is
}

pub fn compile_to_string(p: &SurfProg<Span1>) -> Result<String, CompileErr<Span1>> {
    let () = check_prog(p)?;
    let seq_p: Prog<SeqExp<(HashSet<String>, u32)>, u32> = add_tag_sprog(&liveness_p(
        &uniquify_names(&tag_sprog(&seq_prog(&tag_prog(p)))),
    ));
    let mut is = Vec::<Instr>::new();

    for fun in seq_p.funs {
        let mut fun_instrs = compile_fun(&fun.name, &fun.parameters, &fun.body);
        is.append(&mut fun_instrs);
    }

    is.append(&mut compile_main(&seq_p.main));
    Ok(format!(
        "\
        section .text
        global start_here
        extern snake_error, print_snake_val
{}

ovfl_error:
        mov rdi, 0
        call snake_error

arith_error:
        mov rdi, 1
        call snake_error

cmp_error:
        mov rdi, 2
        call snake_error

logic_error:
        mov rdi, 3
        call snake_error

cond_error:
        mov rdi, 4
        call snake_error
",
        instrs_to_string(&is)
    ))
}
