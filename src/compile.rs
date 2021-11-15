use crate::asm::{instrs_to_string, GENERAL_PURPOSE_REGISTERS};
use crate::asm::{Arg32, Arg64, BinArgs, Instr, Loc, MemRef, MovArgs, Reg, Reg32};
use crate::graph::Graph;
use crate::lexer::Span1;
use crate::syntax::{
    add_tag_sprog, tag_prog, tag_sprog, uniquify_names, Exp, FunDecl, ImmExp, Prim1, Prim2, Prog,
    SeqExp, SeqProg, SurfProg,
};
use itertools::Itertools;
use rand::seq::SliceRandom;
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
        Exp::Call(name, exps, ann) => {
            let mut seq_args = vec![];
            for (i, _) in exps.iter().enumerate() {
                let arg_name = format!("#call_{}_{}", ann, i);
                seq_args.push(ImmExp::Var(arg_name));
            }
            let seq_call = SeqExp::Call(name.to_string(), seq_args, ());
            let mut curr = seq_call;
            for (i, e) in exps.iter().enumerate().rev() {
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
            // Get new body
            let s_body = liveness(body, params, live_out);
            let mut ann_body = s_body.ann();

            // Get new binding
            ann_body.remove(var);
            let s_bind = liveness(bound_exp, params, ann_body.clone());
            let ann_bind = s_bind.ann();

            // Build the new SeqExp::Let
            let mut ann_let = ann_bind;
            ann_let = ann_let
                .difference(params)
                .into_iter()
                .map(|ann| ann.clone())
                .collect();
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
            let ann_thn = s_thn.ann();
            let ann_els = s_els.ann();

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

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, iter::FromIterator};

    use crate::{
        compile::liveness,
        syntax::{ImmExp, Prim2, SeqExp},
    };

    #[test]
    fn liveness_test1() {
        let sexp_orig = SeqExp::Let {
            var: "x".to_string(),
            bound_exp: Box::new(SeqExp::Imm(ImmExp::Num(3), ())),
            body: Box::new(SeqExp::Call(
                "f".to_string(),
                vec![
                    ImmExp::Var("x".to_string()),
                    ImmExp::Var("y".to_string()),
                    ImmExp::Var("a".to_string()),
                ],
                (),
            )),
            ann: (),
        };
        let sexp_hash = liveness(
            &sexp_orig,
            &HashSet::from_iter(vec!["a".to_string()]),
            HashSet::new(),
        );
        let sexp_expt = SeqExp::Let {
            var: "x".to_string(),
            bound_exp: Box::new(SeqExp::Imm(
                ImmExp::Num(3),
                HashSet::from_iter(vec!["y".to_string()]),
            )),
            body: Box::new(SeqExp::Call(
                "f".to_string(),
                vec![
                    ImmExp::Var("x".to_string()),
                    ImmExp::Var("y".to_string()),
                    ImmExp::Var("a".to_string()),
                ],
                HashSet::from_iter(vec!["x".to_string(), "y".to_string()]),
            )),
            ann: HashSet::from_iter(vec!["y".to_string()]),
        };
        assert_eq!(sexp_hash, sexp_expt);
    }

    #[test]
    fn liveness_test2() {
        let sexp_orig = SeqExp::Let {
            var: "y".to_string(),
            bound_exp: Box::new(SeqExp::Imm(ImmExp::Num(1), ())),
            body: Box::new(SeqExp::Let {
                var: "x".to_string(),
                bound_exp: Box::new(SeqExp::Imm(ImmExp::Num(2), ())),
                body: Box::new(SeqExp::Call(
                    "g".to_string(),
                    vec![ImmExp::Var("x".to_string()), ImmExp::Var("y".to_string())],
                    (),
                )),
                ann: (),
            }),
            ann: (),
        };
        let sexp_hash = liveness(&sexp_orig, &HashSet::new(), HashSet::new());
        let sexp_expt = SeqExp::Let {
            var: "y".to_string(),
            bound_exp: Box::new(SeqExp::Imm(ImmExp::Num(1), HashSet::from_iter(vec![]))),
            body: Box::new(SeqExp::Let {
                var: "x".to_string(),
                bound_exp: Box::new(SeqExp::Imm(
                    ImmExp::Num(2),
                    HashSet::from_iter(vec!["y".to_string()]),
                )),
                body: Box::new(SeqExp::Call(
                    "g".to_string(),
                    vec![ImmExp::Var("x".to_string()), ImmExp::Var("y".to_string())],
                    HashSet::from_iter(vec!["x".to_string(), "y".to_string()]),
                )),
                ann: HashSet::from_iter(vec!["y".to_string()]),
            }),
            ann: HashSet::from_iter(vec![]),
        };
        assert_eq!(sexp_hash, sexp_expt);
    }

    #[test]
    fn liveness_test3() {
        let sexp_orig = SeqExp::Let {
            var: "x".to_string(),
            bound_exp: Box::new(SeqExp::Prim2(
                Prim2::Add,
                ImmExp::Var("a".to_string()),
                ImmExp::Var("b".to_string()),
                (),
            )),
            body: Box::new(SeqExp::Let {
                var: "y".to_string(),
                bound_exp: Box::new(SeqExp::Call(
                    "g".to_string(),
                    vec![ImmExp::Var("x".to_string())],
                    (),
                )),
                body: Box::new(SeqExp::Let {
                    var: "z".to_string(),
                    bound_exp: Box::new(SeqExp::Prim2(
                        Prim2::Mul,
                        ImmExp::Var("x".to_string()),
                        ImmExp::Var("y".to_string()),
                        (),
                    )),
                    body: Box::new(SeqExp::Call(
                        "h".to_string(),
                        vec![ImmExp::Var("x".to_string()), ImmExp::Var("z".to_string())],
                        (),
                    )),
                    ann: (),
                }),
                ann: (),
            }),
            ann: (),
        };
        let sexp_hash = liveness(
            &sexp_orig,
            &HashSet::from_iter(vec!["a".to_string(), "b".to_string()]),
            HashSet::new(),
        );
        let sexp_expt = SeqExp::Let {
            var: "x".to_string(),
            bound_exp: Box::new(SeqExp::Prim2(
                Prim2::Add,
                ImmExp::Var("a".to_string()),
                ImmExp::Var("b".to_string()),
                HashSet::new(),
            )),
            body: Box::new(SeqExp::Let {
                var: "y".to_string(),
                bound_exp: Box::new(SeqExp::Call(
                    "g".to_string(),
                    vec![ImmExp::Var("x".to_string())],
                    HashSet::from_iter(vec!["x".to_string()]),
                )),
                body: Box::new(SeqExp::Let {
                    var: "z".to_string(),
                    bound_exp: Box::new(SeqExp::Prim2(
                        Prim2::Mul,
                        ImmExp::Var("x".to_string()),
                        ImmExp::Var("y".to_string()),
                        HashSet::from_iter(vec!["x".to_string(), "y".to_string()]),
                    )),
                    body: Box::new(SeqExp::Call(
                        "h".to_string(),
                        vec![ImmExp::Var("x".to_string()), ImmExp::Var("z".to_string())],
                        HashSet::from_iter(vec!["x".to_string(), "z".to_string()]),
                    )),
                    ann: HashSet::from_iter(vec!["x".to_string(), "y".to_string()]),
                }),
                ann: HashSet::from_iter(vec!["x".to_string()]),
            }),
            ann: HashSet::new(),
        };
        assert_eq!(sexp_hash, sexp_expt);
    }

    #[test]
    fn liveness_test4() {
        let sexp_orig = SeqExp::Let {
            var: "x".to_string(),
            bound_exp: Box::new(SeqExp::Prim2(
                Prim2::Add,
                ImmExp::Var("a".to_string()),
                ImmExp::Var("b".to_string()),
                (),
            )),
            body: Box::new(SeqExp::Let {
                var: "y".to_string(),
                bound_exp: Box::new(SeqExp::Call(
                    "g".to_string(),
                    vec![ImmExp::Var("b".to_string())],
                    (),
                )),
                body: Box::new(SeqExp::Let {
                    var: "z".to_string(),
                    bound_exp: Box::new(SeqExp::If {
                        cond: ImmExp::Var("b".to_string()),
                        thn: Box::new(SeqExp::Prim2(
                            Prim2::Add,
                            ImmExp::Var("x".to_string()),
                            ImmExp::Num(1),
                            (),
                        )),
                        els: Box::new(SeqExp::Imm(ImmExp::Var("y".to_string()), ())),
                        ann: (),
                    }),
                    body: Box::new(SeqExp::Call(
                        "h".to_string(),
                        vec![ImmExp::Var("z".to_string())],
                        (),
                    )),
                    ann: (),
                }),
                ann: (),
            }),
            ann: (),
        };
        let sexp_hash = liveness(
            &sexp_orig,
            &HashSet::from_iter(vec!["a".to_string(), "b".to_string()]),
            HashSet::new(),
        );
        let sexp_expt = SeqExp::Let {
            var: "x".to_string(),
            bound_exp: Box::new(SeqExp::Prim2(
                Prim2::Add,
                ImmExp::Var("a".to_string()),
                ImmExp::Var("b".to_string()),
                HashSet::new(),
            )),
            body: Box::new(SeqExp::Let {
                var: "y".to_string(),
                bound_exp: Box::new(SeqExp::Call(
                    "g".to_string(),
                    vec![ImmExp::Var("b".to_string())],
                    HashSet::from_iter(vec!["x".to_string()]),
                )),
                body: Box::new(SeqExp::Let {
                    var: "z".to_string(),
                    bound_exp: Box::new(SeqExp::If {
                        cond: ImmExp::Var("b".to_string()),
                        thn: Box::new(SeqExp::Prim2(
                            Prim2::Add,
                            ImmExp::Var("x".to_string()),
                            ImmExp::Num(1),
                            HashSet::from_iter(vec!["x".to_string()]),
                        )),
                        els: Box::new(SeqExp::Imm(
                            ImmExp::Var("y".to_string()),
                            HashSet::from_iter(vec!["y".to_string()]),
                        )),
                        ann: HashSet::from_iter(vec!["x".to_string(), "y".to_string()]),
                    }),
                    body: Box::new(SeqExp::Call(
                        "h".to_string(),
                        vec![ImmExp::Var("z".to_string())],
                        HashSet::from_iter(vec!["z".to_string()]),
                    )),
                    ann: HashSet::from_iter(vec!["x".to_string(), "y".to_string()]),
                }),
                ann: HashSet::from_iter(vec!["x".to_string()]),
            }),
            ann: HashSet::new(),
        };
        assert_eq!(sexp_hash, sexp_expt);
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
            ann: _,
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

            // Get body's conflict HashSet
            let mut conf_combs = Vec::new();
            let conf_set = body.map_ann(&mut |ann| ann.0.clone()).ann();
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

pub fn allocate_registers(
    mut conflicts: Graph<String>,
    registers: &[Reg],
) -> HashMap<String, VarLocation> {
    // let mut env = HashMap::<String, VarLocation>::new();
    // for (i, v) in conflicts.vertices().into_iter().enumerate() {
    //     env.insert(v, VarLocation::Spill(i as i32));
    // }
    // env

    println!("{:?}", registers);

    // Utility
    let mut rng = rand::thread_rng();
    let regs_set: HashSet<&Reg> = HashSet::from_iter(registers);

    // Initialize env
    let mut spill_cnt = 0;
    let mut env = HashMap::<String, VarLocation>::new();

    // Find vertices that has edges more than the number of available registers
    let reg_num = registers.len();
    loop {
        match conflicts
            .vertices()
            .into_iter()
            .find(|v| conflicts.neighbors(&v).unwrap().len() >= reg_num)
        {
            Some(vtx) => {
                conflicts.remove_vertex(&vtx);
                env.insert(vtx, VarLocation::Spill(spill_cnt));
                spill_cnt += 1;
            }
            None => break,
        };
    }

    // While G cannot be R-colored
    //     While graph G has a node N with degree less than R
    //         Remove N and its associated edges from G and push N on a stack S
    //     End While
    //
    //     While stack S contains a node N
    //         Add N to graph G and assign it a color from the R colors
    //         If failed, Simplify the graph G by choosing an object to spill and remove its node N from G
    //         (spill nodes are chosen based on object’s number of definitions and references)
    //         break the while
    //     End while
    // End While

    // Try to color the graph
    loop {
        let mut curr_map: HashMap<String, Reg> = HashMap::new();

        // (Remove vertices and) push onto the stack
        let mut stack = conflicts.vertices();
        stack.shuffle(&mut rng);

        for v in stack {
            let used_regs = match conflicts.neighbors(&v) {
                Some(nbrs) => nbrs
                    .into_iter()
                    .map(|nbr| curr_map.get(nbr).unwrap_or(&Reg::Rax))
                    .collect(),
                None => HashSet::new(),
            };
            match regs_set
                .difference(&used_regs)
                .into_iter()
                .map(|reg| *reg.clone())
                .collect::<Vec<Reg>>()
                .choose(&mut rng)
            {
                Some(reg) => {
                    curr_map.insert(v, *reg);
                }
                None => break,
            }
        }

        // Merge
        for (k, v) in curr_map {
            env.insert(k, VarLocation::Reg(v));
        }
        break;
    }
    env
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
    is.append(&mut compile_to_instrs(&Vec::new(), &env_lcl, &e));
    is
}

fn check_isbool(dest: Reg, lab: &str) -> Vec<Instr> {
    let mut instrs = vec![];
    if DEBUG_MODE {
        return instrs;
    }
    instrs.push(Instr::Mov(MovArgs::ToReg(Reg::Rsi, Arg64::Reg(dest))));
    instrs.push(Instr::Test(BinArgs::ToReg(
        Reg::Rsi,
        Arg32::Unsigned(BOOL_TAG32),
    )));
    instrs.push(Instr::Jz(lab.to_string()));
    instrs
}

fn check_isnum(dest: Reg, lab: &str) -> Vec<Instr> {
    let mut instrs = vec![];
    if DEBUG_MODE {
        return instrs;
    }
    instrs.push(Instr::Mov(MovArgs::ToReg(Reg::Rsi, Arg64::Reg(dest))));
    instrs.push(Instr::Test(BinArgs::ToReg(
        Reg::Rsi,
        Arg32::Unsigned(BOOL_TAG32),
    )));
    instrs.push(Instr::Jnz(lab.to_string()));
    instrs
}

fn check_overflow() -> Vec<Instr> {
    let mut instrs = vec![];
    if DEBUG_MODE {
        return instrs;
    }
    instrs.push(Instr::Jo("ovfl_error".to_string()));
    instrs
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
                            offset: (n + 3) * 8,
                        }),
                    ))]
                }
                None => panic!("Cannot get variable"),
            }
        }
    }
}

fn compile_with_env<'exp>(
    e: &'exp SeqExp<u32>,
    env_arg: &Vec<(&'exp str, i32)>,
    env_lcl: &HashMap<String, VarLocation>,
    dest: Reg,
    is_tail: bool,
) -> Vec<Instr> {
    match e {
        SeqExp::Imm(imm, _) => compile_imm(imm, env_arg, env_lcl, dest),
        SeqExp::Prim1(prim1, imm, ann) => {
            let mut instrs = compile_imm(imm, env_arg, env_lcl, dest);
            match prim1 {
                Prim1::Add1 => {
                    instrs.append(&mut check_isnum(dest, "arith_error"));
                    instrs.push(Instr::Add(BinArgs::ToReg(dest, Arg32::Signed(1 << 1))));
                    instrs.append(&mut check_overflow());
                }
                Prim1::Sub1 => {
                    instrs.append(&mut check_isnum(dest, "arith_error"));
                    instrs.push(Instr::Sub(BinArgs::ToReg(dest, Arg32::Signed(1 << 1))));
                    instrs.append(&mut check_overflow());
                }
                Prim1::Not => {
                    assert!(dest != Reg::R15);
                    instrs.append(&mut check_isbool(dest, "logic_error"));
                    instrs.push(Instr::Mov(MovArgs::ToReg(
                        Reg::R15,
                        Arg64::Unsigned(BOOL_MASK),
                    )));
                    instrs.push(Instr::Xor(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                }
                Prim1::Print => {
                    // mov rdi, rax
                    // call print_snake_val
                    instrs.push(Instr::Mov(MovArgs::ToReg(Reg::Rdi, Arg64::Reg(dest))));
                    instrs.push(Instr::Call("print_snake_val".to_string()));
                }
                Prim1::IsBool => {
                    //     mov r15, bool_tag
                    //     test rax, r15
                    //     mov rax, true
                    //     jnz isbool
                    //     mov rax, false
                    // isbool:
                    let isbool_lab = format!("isbool#{}", ann);
                    instrs.push(Instr::Mov(MovArgs::ToReg(
                        Reg::R15,
                        Arg64::Unsigned(BOOL_TAG),
                    )));
                    instrs.push(Instr::Test(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Jnz(isbool_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(isbool_lab));
                }
                Prim1::IsNum => {
                    //     mov r15, bool_tag
                    //     test rax, r15
                    //     mov rax, true
                    //     jz isnum
                    //     mov rax, false
                    // isnum:
                    let isnum_lab = format!("isnum#{}", ann);
                    instrs.push(Instr::Mov(MovArgs::ToReg(
                        Reg::R15,
                        Arg64::Unsigned(BOOL_TAG),
                    )));
                    instrs.push(Instr::Test(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Jz(isnum_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(isnum_lab));
                }
                Prim1::PrintStack => todo!(),
            }
            instrs
        }
        SeqExp::Prim2(prim2, imm1, imm2, ann) => {
            assert!(dest != Reg::R15);

            // Compile imm1 with destination register RAX
            let mut instrs = compile_imm(imm1, env_arg, env_lcl, dest);

            // Compile imm2 with destination register R15
            instrs.append(&mut compile_imm(imm2, env_arg, env_lcl, Reg::R15));

            match prim2 {
                Prim2::Add => {
                    instrs.append(&mut check_isnum(dest, "arith_error"));
                    instrs.append(&mut check_isnum(Reg::R15, "arith_error"));
                    instrs.push(Instr::Add(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.append(&mut check_overflow());
                }
                Prim2::Sub => {
                    instrs.append(&mut check_isnum(dest, "arith_error"));
                    instrs.append(&mut check_isnum(Reg::R15, "arith_error"));
                    instrs.push(Instr::Sub(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.append(&mut check_overflow());
                }
                Prim2::Mul => {
                    instrs.append(&mut check_isnum(dest, "arith_error"));
                    instrs.append(&mut check_isnum(Reg::R15, "arith_error"));
                    instrs.push(Instr::Sar(BinArgs::ToReg(dest, Arg32::Signed(1))));
                    instrs.push(Instr::IMul(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.append(&mut check_overflow());
                }
                Prim2::And => {
                    instrs.append(&mut check_isbool(dest, "logic_error"));
                    instrs.append(&mut check_isbool(Reg::R15, "logic_error"));
                    instrs.push(Instr::And(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                }
                Prim2::Or => {
                    instrs.append(&mut check_isbool(dest, "logic_error"));
                    instrs.append(&mut check_isbool(Reg::R15, "logic_error"));
                    instrs.push(Instr::Or(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                }
                Prim2::Lt => {
                    let lt_lab = format!("lt#{}", ann);
                    instrs.append(&mut check_isnum(dest, "cmp_error"));
                    instrs.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    instrs.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Jl(lt_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(lt_lab));
                }
                Prim2::Gt => {
                    let gt_lab = format!("gt#{}", ann);
                    instrs.append(&mut check_isnum(dest, "cmp_error"));
                    instrs.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    instrs.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Jg(gt_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(gt_lab));
                }
                Prim2::Le => {
                    let le_lab = format!("le#{}", ann);
                    instrs.append(&mut check_isnum(dest, "cmp_error"));
                    instrs.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    instrs.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Jle(le_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(le_lab));
                }
                Prim2::Ge => {
                    let ge_lab = format!("ge#{}", ann);
                    instrs.append(&mut check_isnum(dest, "cmp_error"));
                    instrs.append(&mut check_isnum(Reg::R15, "cmp_error"));
                    instrs.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Jge(ge_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(ge_lab));
                }
                Prim2::Eq => {
                    let eq_lab = format!("eq#{}", ann);
                    instrs.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Je(eq_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(eq_lab));
                }
                Prim2::Neq => {
                    let neq_lab = format!("neq#{}", ann);
                    instrs.push(Instr::Cmp(BinArgs::ToReg(dest, Arg32::Reg(Reg::R15))));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_TRU))));
                    instrs.push(Instr::Jne(neq_lab.clone()));
                    instrs.push(Instr::Mov(MovArgs::ToReg(dest, Arg64::Unsigned(SNAKE_FLS))));
                    instrs.push(Instr::Label(neq_lab));
                }
            };
            instrs
        }
        SeqExp::Let {
            var,
            bound_exp,
            body,
            ann: _,
        } => {
            let mut instrs = compile_with_env(bound_exp, env_arg, env_lcl, dest, false);

            // Save RAX to stack memory or registers
            match env_lcl.get(var) {
                Some(vl) => match vl {
                    VarLocation::Reg(r) => {
                        instrs.push(Instr::Mov(MovArgs::ToReg(*r, Arg64::Reg(dest))));
                    }
                    VarLocation::Spill(n) => {
                        instrs.push(Instr::Mov(MovArgs::ToMem(
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

            instrs.append(&mut compile_with_env(body, env_arg, env_lcl, dest, is_tail));
            instrs
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
            is.extend(compile_with_env(thn, env_arg, env_lcl, dest, is_tail));

            // Jump to done
            is.push(Instr::Jmp(done_lab.clone()));

            // False label
            is.push(Instr::Label(else_lab));

            // Compile else expression
            is.extend(compile_with_env(els, env_arg, env_lcl, dest, is_tail));

            // Done label
            is.push(Instr::Label(done_lab));

            is
        }
        SeqExp::Call(name, imms, _) => {
            let mut is = vec![];

            // Count callee and caller's #args
            let callee_arg_num = imms.len();
            let caller_arg_num = env_arg.len();

            if is_tail && callee_arg_num <= caller_arg_num {
                // Overwrite our parameters with the callee's parameters
                for (i, imm) in imms.iter().enumerate() {
                    is.append(&mut compile_imm(imm, env_arg, env_lcl, dest));
                    is.push(Instr::Mov(MovArgs::ToMem(
                        MemRef {
                            reg: Reg::Rbp,
                            offset: (i as i32 + 3) * 8,
                        },
                        Reg32::Reg(Reg::Rax),
                    )));
                }
                // Make the stack pointer point at base pointer
                is.push(Instr::Mov(MovArgs::ToReg(Reg::Rsp, Arg64::Reg(Reg::Rbp))));
                // Restore R15 and Rbp
                is.push(Instr::Pop(Loc::Reg(Reg::R15)));
                is.push(Instr::Pop(Loc::Reg(Reg::Rbp)));
                // Jump to the function
                is.push(Instr::Jmp(name.to_string()));
            } else {
                let offset = -8 * imms.len() as i32;
                for imm in imms.iter().rev() {
                    is.append(&mut compile_imm(imm, env_arg, env_lcl, dest));
                    is.push(Instr::Push(Arg32::Reg(dest)));
                }
                is.push(Instr::Call(name.to_string()));
                is.push(Instr::Sub(BinArgs::ToReg(Reg::Rsp, Arg32::Signed(offset))));
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

    let mut instrs = vec![];
    instrs.push(Instr::Push(Arg32::Reg(Reg::Rbp)));
    instrs.push(Instr::Push(Arg32::Reg(Reg::R15)));
    instrs.push(Instr::Mov(MovArgs::ToReg(Reg::Rbp, Arg64::Reg(Reg::Rsp))));
    instrs.push(Instr::Sub(BinArgs::ToReg(
        Reg::Rsp,
        Arg32::Signed(space * 8),
    )));
    instrs.append(&mut compile_with_env(e, env_arg, env_lcl, Reg::Rax, true));
    instrs.push(Instr::Mov(MovArgs::ToReg(Reg::Rsp, Arg64::Reg(Reg::Rbp))));
    instrs.push(Instr::Pop(Loc::Reg(Reg::R15)));
    instrs.push(Instr::Pop(Loc::Reg(Reg::Rbp)));
    instrs.push(Instr::Ret);
    instrs
}

pub fn compile_to_string(p: &SurfProg<Span1>) -> Result<String, CompileErr<Span1>> {
    let () = check_prog(p)?;
    let seq_p: Prog<SeqExp<(HashSet<String>, u32)>, u32> = add_tag_sprog(&liveness_p(
        &uniquify_names(&tag_sprog(&seq_prog(&tag_prog(p)))),
    ));
    let mut instrs = Vec::<Instr>::new();

    for fun in seq_p.funs {
        let mut fun_instrs = compile_fun(&fun.name, &fun.parameters, &fun.body);
        instrs.append(&mut fun_instrs);
    }

    instrs.append(&mut compile_main(&seq_p.main));
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
        instrs_to_string(&instrs)
    ))
}
