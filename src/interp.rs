use crate::syntax::{Exp, Prim1, Prim2, SurfFunDecl, SurfProg};

use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SnakeVal {
    Num(i64), // should fit into 63 bits though
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpErr {
    ExpectedNum {
        who: String,
        got: SnakeVal,
        msg: String,
    },
    ExpectedBool {
        who: String,
        got: SnakeVal,
        msg: String,
    },
    Overflow {
        msg: String,
    },
    Write {
        msg: String,
    },
}

type Interp<T> = Result<T, InterpErr>;

use std::fmt;
use std::fmt::Display;

impl Display for SnakeVal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SnakeVal::Num(n) => write!(f, "{}", n),
            SnakeVal::Bool(b) => write!(f, "{}", b),
        }
    }
}

impl Display for InterpErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InterpErr::ExpectedNum { who, got: v, msg } => {
                write!(f, "{} expected a number, but got {} in {}", who, v, msg)
            }
            InterpErr::ExpectedBool { who, got: v, msg } => {
                write!(f, "{} expected a boolean, but got {} in {}", who, v, msg)
            }
            InterpErr::Overflow { msg } => write!(f, "Operation {} overflowed", msg),
            InterpErr::Write { msg } => write!(f, "I/O Error when printing: {}", msg),
        }
    }
}

fn get<T>(stk: &List<(&str, T)>, x: &str) -> Option<T>
where
    T: Copy,
{
    match stk {
        List::Empty => None,
        List::Cons((y, n), stk) => {
            if x == *y {
                Some(*n)
            } else {
                get(stk, x)
            }
        }
    }
}

fn bool(v: SnakeVal, who: &str, msg: &str) -> Interp<bool> {
    match v {
        SnakeVal::Bool(b) => Ok(b),
        _ => Err(InterpErr::ExpectedBool {
            who: String::from(who),
            got: v,
            msg: String::from(msg),
        }),
    }
}

fn num(v: SnakeVal, who: &str, msg: &str) -> Interp<i64> {
    match v {
        SnakeVal::Num(n) => Ok(n),
        _ => Err(InterpErr::ExpectedNum {
            who: String::from(who),
            got: v,
            msg: String::from(msg),
        }),
    }
}

fn interpret_prim1<W>(p: &Prim1, w: &mut W, v: SnakeVal) -> Interp<SnakeVal>
where
    W: std::io::Write,
{
    match p {
        Prim1::Add1 => snake_arith(v, SnakeVal::Num(1), |n1, n2| n1.overflowing_add(n2), "add1"),
        Prim1::Sub1 => snake_arith(v, SnakeVal::Num(1), |n1, n2| n1.overflowing_sub(n2), "sub1"),
        Prim1::Not => Ok(SnakeVal::Bool(!bool(v, "logic", "!")?)),
        Prim1::Print | Prim1::PrintStack => {
            writeln!(w, "{}", v).map_err(|e| InterpErr::Write {
                msg: format!("{}", e),
            })?;
            Ok(v)
        }
        Prim1::IsBool => match v {
            SnakeVal::Bool(_) => Ok(SnakeVal::Bool(true)),
            _ => Ok(SnakeVal::Bool(false)),
        },
        Prim1::IsNum => match v {
            SnakeVal::Num(_) => Ok(SnakeVal::Bool(true)),
            _ => Ok(SnakeVal::Bool(false)),
        },
    }
}

static MAX_INT: i64 = 2i64.pow(62) - 1;
static MIN_INT: i64 = -(2i64.pow(62));
fn out_of_bounds(n: i64) -> bool {
    n > MAX_INT || n < MIN_INT
}

fn snake_arith<F>(v1: SnakeVal, v2: SnakeVal, arith: F, op: &str) -> Interp<SnakeVal>
where
    F: Fn(i64, i64) -> (i64, bool),
{
    let n1 = num(v1, "arithmetic", op)?;
    let n2 = num(v2, "arithmetic", op)?;
    let (n3, overflow) = arith(n1, n2);
    if overflow || out_of_bounds(n3) {
        Err(InterpErr::Overflow {
            msg: format!("{} {} {} = {}", n1, op, n2, n3),
        })
    } else {
        Ok(SnakeVal::Num(n3))
    }
}

fn snake_log<F>(v1: SnakeVal, v2: SnakeVal, log: F, op: &str) -> Interp<SnakeVal>
where
    F: Fn(bool, bool) -> bool,
{
    Ok(SnakeVal::Bool(log(
        bool(v1, "logic", op)?,
        bool(v2, "logic", op)?,
    )))
}

fn snake_cmp<F>(v1: SnakeVal, v2: SnakeVal, cmp: F, op: &str) -> Interp<SnakeVal>
where
    F: Fn(i64, i64) -> bool,
{
    Ok(SnakeVal::Bool(cmp(
        num(v1, "comparison", op)?,
        num(v2, "comparison", op)?,
    )))
}

fn interpret_prim2(p: &Prim2, v1: SnakeVal, v2: SnakeVal) -> Interp<SnakeVal> {
    match p {
        Prim2::Add => snake_arith(v1, v2, |n1, n2| n1.overflowing_add(n2), "+"),
        Prim2::Sub => snake_arith(v1, v2, |n1, n2| n1.overflowing_sub(n2), "-"),
        Prim2::Mul => snake_arith(v1, v2, |n1, n2| n1.overflowing_mul(n2), "*"),

        Prim2::And => snake_log(v1, v2, |b1, b2| b1 && b2, "&&"),
        Prim2::Or => snake_log(v1, v2, |b1, b2| b1 || b2, "||"),

        Prim2::Lt => snake_cmp(v1, v2, |n1, n2| n1 < n2, "<"),
        Prim2::Le => snake_cmp(v1, v2, |n1, n2| n1 <= n2, "<="),
        Prim2::Gt => snake_cmp(v1, v2, |n1, n2| n1 > n2, ">"),
        Prim2::Ge => snake_cmp(v1, v2, |n1, n2| n1 >= n2, ">="),

        Prim2::Eq => Ok(SnakeVal::Bool(v1 == v2)),
        Prim2::Neq => Ok(SnakeVal::Bool(v1 != v2)),
    }
}

// enum STK<'s, T> {
//     EOS,
//     Prim1(Prim1, Box<STK<'s, T>>),
//     Prim2L(Prim2, &'s Exp<T>, ENV, Box<STK<'s, T>>),
//     Prim2R(Prim2, SnakeVal, Box<STK<'s, T>>),
//     If(&'s Exp<T>, &'s Exp<T>,  ENV, Box<STK<'s, T>>),
//     Let(&'s BindExpr, Vec<(&'s BindExpr, &'s Exp<T>)>, &'s Exp<T>, ENV, Box<STK<'s, T>>),
//     App(String, Vec<SnakeVal>, Vec<&'s Exp<T>>, ENV, Box<STK<'s, T>>),
// }

// A reference-counted linked list/the functional programmer's List
enum List<T> {
    Empty,
    Cons(T, Rc<List<T>>),
}

impl<A> std::iter::FromIterator<A> for List<A> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = A>,
    {
        let mut l = List::Empty;
        for t in iter {
            l = List::Cons(t, Rc::new(l));
        }
        l
    }
}

// Abstract machine
enum Machine<'exp, Ann> {
    Descending {
        e: &'exp Exp<Ann>,
        stk: Stack<'exp, Ann>,
        env: Env<'exp>,
    },
    Returning {
        v: SnakeVal,
        stk: Stack<'exp, Ann>,
    },
}

type Env<'exp> = Rc<List<(&'exp str, SnakeVal)>>;

struct Closure<'exp, Ann> {
    exp: &'exp Exp<Ann>,
    env: Env<'exp>,
}

enum Stack<'exp, Ann> {
    Done,
    Prim1(Prim1, Box<Stack<'exp, Ann>>),
    Prim2L(Prim2, Closure<'exp, Ann>, Box<Stack<'exp, Ann>>),
    Prim2R(Prim2, SnakeVal, Box<Stack<'exp, Ann>>),
    If {
        thn: &'exp Exp<Ann>,
        els: &'exp Exp<Ann>,
        env: Env<'exp>,
        stk: Box<Stack<'exp, Ann>>,
    },
    Let {
        var: &'exp str,
        env: Env<'exp>,
        bindings: Vec<&'exp (String, Exp<Ann>)>,
        body: &'exp Exp<Ann>,
        stk: Box<Stack<'exp, Ann>>,
    },
    Call {
        fun: &'exp str,
        evaled_args: Vec<SnakeVal>,
        env: Env<'exp>,
        remaining_args: Vec<&'exp Exp<Ann>>,
        stk: Box<Stack<'exp, Ann>>,
    },
}

type Funs<'e, Ann> = HashMap<&'e str, (&'e [String], &'e Exp<Ann>)>;

/*
 *  Abstract machine-style interpreter.
 *
 *  Defunctionalizes the kontinuation of the direct-style interpreter
 *  so that we don't blow the Rust stack/rely on Rust TCE.
 *
*/
fn machine<Ann, W>(e: &Exp<Ann>, decls: &Funs<Ann>, buf: &mut W) -> Interp<SnakeVal>
where
    W: std::io::Write,
{
    fn call<'exp, Ann>(
        fun: &str,
        args: Vec<SnakeVal>,
        decls: &Funs<'exp, Ann>,
        stk: Stack<'exp, Ann>,
    ) -> Interp<Machine<'exp, Ann>> {
        let (p_names, body) = decls
            .get(fun)
            .ok_or_else(|| panic!("unbound function in interpreter: {}", fun))?;
        if args.len() != p_names.len() {
            panic!(
                "function {} called with {} inputs but expects {}",
                fun,
                args.len(),
                p_names.len()
            );
        }
        let env = args
            .iter()
            .zip(p_names.iter())
            .map(|(v, s)| (s.as_str(), *v))
            .collect();
        Ok(Machine::Descending {
            e: body,
            env: Rc::new(env),
            stk,
        })
    }

    let mut machine = Machine::Descending {
        e,
        stk: Stack::Done,
        env: Rc::new(List::Empty),
    };
    loop {
        match machine {
            Machine::Descending { e, stk, env } => match e {
                Exp::Num(n, _) => {
                    machine = Machine::Returning {
                        v: SnakeVal::Num(*n),
                        stk,
                    }
                }
                Exp::Bool(b, _) => {
                    machine = Machine::Returning {
                        v: SnakeVal::Bool(*b),
                        stk,
                    }
                }
                Exp::Var(x, _) => {
                    let v = get(&env, x).expect("Unbound variable in interpreter! You should catch this in the check function!");
                    machine = Machine::Returning { v, stk }
                }
                Exp::Prim1(op, e, _) => {
                    machine = Machine::Descending {
                        e,
                        stk: Stack::Prim1(*op, Box::new(stk)),
                        env,
                    };
                }
                Exp::Prim2(op, e1, e2, _) => {
                    machine = Machine::Descending {
                        e: e1,
                        stk: Stack::Prim2L(
                            *op,
                            Closure {
                                exp: e2,
                                env: env.clone(),
                            },
                            Box::new(stk),
                        ),
                        env,
                    };
                }
                Exp::Let { bindings, body, .. } => {
                    let mut rbindings: Vec<&(String, Exp<Ann>)> = bindings.iter().rev().collect();
                    match rbindings.pop() {
                        None => {
                            machine = Machine::Descending { e: body, stk, env };
                        }
                        Some((var, e)) => {
                            machine = Machine::Descending {
                                e,
                                stk: Stack::Let {
                                    var,
                                    env: env.clone(),
                                    bindings: rbindings,
                                    body,
                                    stk: Box::new(stk),
                                },
                                env,
                            };
                        }
                    }
                }
                Exp::If { cond, thn, els, .. } => {
                    machine = Machine::Descending {
                        e: cond,
                        stk: Stack::If {
                            thn,
                            els,
                            env: env.clone(),
                            stk: Box::new(stk),
                        },
                        env,
                    }
                }
                Exp::Call(fun, args, _) => {
                    let mut rargs: Vec<&Exp<_>> = args.iter().rev().collect();
                    machine = match rargs.pop() {
                        None => call(fun, vec![], decls, stk)?,
                        Some(e) => Machine::Descending {
                            e,
                            stk: Stack::Call {
                                fun,
                                evaled_args: Vec::new(),
                                env: env.clone(),
                                remaining_args: rargs,
                                stk: Box::new(stk),
                            },
                            env,
                        },
                    }
                }
            },
            Machine::Returning { v, stk } => match stk {
                Stack::Done => return Ok(v),
                Stack::Prim1(op, stk) => {
                    let v = interpret_prim1(&op, buf, v)?;
                    machine = Machine::Returning { v, stk: *stk }
                }
                Stack::Prim2L(op, r, stk) => {
                    machine = Machine::Descending {
                        e: r.exp,
                        env: r.env,
                        stk: Stack::Prim2R(op, v, stk),
                    };
                }
                Stack::Prim2R(op, vl, stk) => {
                    let v = interpret_prim2(&op, vl, v)?;
                    machine = Machine::Returning { v, stk: *stk };
                }
                Stack::Let {
                    var,
                    mut env,
                    mut bindings,
                    body,
                    stk,
                } => {
                    env = Rc::new(List::Cons((var, v), env));
                    machine = match bindings.pop() {
                        None => Machine::Descending {
                            e: body,
                            env,
                            stk: *stk,
                        },
                        Some((var, e)) => Machine::Descending {
                            e,
                            stk: Stack::Let {
                                var,
                                env: env.clone(),
                                bindings,
                                body,
                                stk,
                            },
                            env,
                        },
                    }
                }
                Stack::If { thn, els, env, stk } => {
                    let e = if bool(v, "if", "if")? { thn } else { els };
                    machine = Machine::Descending { e, env, stk: *stk }
                }
                Stack::Call {
                    fun,
                    mut evaled_args,
                    env,
                    mut remaining_args,
                    stk,
                } => {
                    evaled_args.push(v);
                    match remaining_args.pop() {
                        None => {
                            machine = call(fun, evaled_args, decls, *stk)?;
                        }
                        Some(e) => {
                            machine = Machine::Descending {
                                e,
                                env: env.clone(),
                                stk: Stack::Call {
                                    fun,
                                    evaled_args,
                                    env,
                                    remaining_args,
                                    stk,
                                },
                            }
                        }
                    }
                }
            },
        }
    }
}

// Runs the reference interpreter.
pub fn exp<Ann, W>(e: &Exp<Ann>, w: &mut W) -> Interp<SnakeVal>
where
    W: std::io::Write,
{
    machine(e, &HashMap::new(), w)
}

fn decls_to_hash<Ann>(decls: &[SurfFunDecl<Ann>]) -> Funs<Ann> {
    let mut h = HashMap::new();
    for d in decls {
        h.insert(d.name.as_str(), (d.parameters.as_slice(), &d.body));
    }
    h
}

pub fn prog<Ann, W>(p: &SurfProg<Ann>, w: &mut W) -> Interp<SnakeVal>
where
    W: std::io::Write,
{
    let funs = decls_to_hash(&p.funs);
    machine(&p.main, &funs, w)
}
