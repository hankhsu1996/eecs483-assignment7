use crate::asm::{instrs_to_string, GENERAL_PURPOSE_REGISTERS};
use crate::asm::{Arg32, Arg64, BinArgs, Instr, Loc, MemRef, MovArgs, Reg, Reg32};
use crate::graph::Graph;
use crate::lexer::Span1;
use crate::syntax::{
    add_tag_sprog, tag_prog, tag_sprog, uniquify_names, Exp, FunDecl, ImmExp, Prim1, Prim2, Prog,
    SeqExp, SeqProg, SurfFunDecl, SurfProg,
};
use itertools::Itertools;
use std::collections::{HashMap, HashSet};

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
