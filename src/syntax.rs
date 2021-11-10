/* Programs */
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prog<E, Ann> {
    pub funs: Vec<FunDecl<E, Ann>>,
    pub main: E,
    pub ann: Ann,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunDecl<E, Ann> {
    pub name: String,
    pub parameters: Vec<String>,
    pub body: E,
    pub ann: Ann,
}

pub type SurfProg<Ann> = Prog<Exp<Ann>, Ann>;
pub type SurfFunDecl<Ann> = FunDecl<Exp<Ann>, Ann>;

pub type SeqProg<Ann> = Prog<SeqExp<Ann>, Ann>;
pub type SeqFunDecl<Ann> = FunDecl<SeqExp<Ann>, Ann>;

/* Expressions */
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exp<Ann> {
    Num(i64, Ann),
    Bool(bool, Ann),
    Var(String, Ann),
    Prim1(Prim1, Box<Exp<Ann>>, Ann),
    Prim2(Prim2, Box<Exp<Ann>>, Box<Exp<Ann>>, Ann),
    Let {
        bindings: Vec<(String, Exp<Ann>)>, // new binding declarations
        body: Box<Exp<Ann>>,               // the expression in which the new variables are bound
        ann: Ann,
    },
    If {
        cond: Box<Exp<Ann>>,
        thn: Box<Exp<Ann>>,
        els: Box<Exp<Ann>>,
        ann: Ann,
    },
    Call(String, Vec<Exp<Ann>>, Ann),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Prim1 {
    Add1,
    Sub1,
    Not,
    Print,
    IsBool,
    IsNum,
    PrintStack,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Prim2 {
    Add,
    Sub,
    Mul,

    And,
    Or,

    Lt,
    Gt,
    Le,
    Ge,

    Eq,
    Neq,
}

/* Sequential Expressions */

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImmExp {
    Num(i64),
    Bool(bool),
    Var(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeqExp<Ann> {
    Imm(ImmExp, Ann),
    Prim1(Prim1, ImmExp, Ann),
    Prim2(Prim2, ImmExp, ImmExp, Ann),
    Let {
        var: String,
        bound_exp: Box<SeqExp<Ann>>,
        body: Box<SeqExp<Ann>>,
        ann: Ann,
    },
    If {
        cond: ImmExp,
        thn: Box<SeqExp<Ann>>,
        els: Box<SeqExp<Ann>>,
        ann: Ann,
    },
    Call(String, Vec<ImmExp>, Ann),
}

/* Useful functions for Exps, SeqExps */
impl<Ann> Exp<Ann> {
    pub fn ann(&self) -> Ann
    where
        Ann: Clone,
    {
        match self {
            Exp::Num(_, a) | Exp::Bool(_, a) => a.clone(),
            Exp::Var(_, a) => a.clone(),
            Exp::Prim1(_, _, a) => a.clone(),
            Exp::Prim2(_, _, _, a) => a.clone(),
            Exp::Let { ann: a, .. } => a.clone(),
            Exp::If { ann: a, .. } => a.clone(),
            Exp::Call(_, _, a) => a.clone(),
        }
    }

    pub fn ann_mut(&mut self) -> &mut Ann {
        match self {
            Exp::Num(_, a) | Exp::Bool(_, a) => a,
            Exp::Var(_, a) => a,
            Exp::Prim1(_, _, a) => a,
            Exp::Prim2(_, _, _, a) => a,
            Exp::Let { ann: a, .. } => a,
            Exp::If { ann: a, .. } => a,
            Exp::Call(_, _, a) => a,
        }
    }

    pub fn map_ann<Ann2, F>(&self, f: &mut F) -> Exp<Ann2>
    where
        F: FnMut(&Ann) -> Ann2,
    {
        match self {
            Exp::Num(n, a) => Exp::Num(*n, f(a)),
            Exp::Bool(b, a) => Exp::Bool(*b, f(a)),
            Exp::Var(s, a) => Exp::Var(s.clone(), f(a)),
            Exp::Prim1(op, e, a) => Exp::Prim1(*op, Box::new(e.map_ann(f)), f(a)),
            Exp::Prim2(op, e1, e2, a) => {
                Exp::Prim2(*op, Box::new(e1.map_ann(f)), Box::new(e2.map_ann(f)), f(a))
            }
            Exp::Let {
                bindings,
                body,
                ann,
            } => Exp::Let {
                bindings: bindings
                    .iter()
                    .map(|(x, e)| (x.clone(), e.map_ann(f)))
                    .collect(),
                body: Box::new(body.map_ann(f)),
                ann: f(ann),
            },
            Exp::If {
                cond,
                thn,
                els,
                ann,
            } => Exp::If {
                cond: Box::new(cond.map_ann(f)),
                thn: Box::new(thn.map_ann(f)),
                els: Box::new(els.map_ann(f)),
                ann: f(ann),
            },
            Exp::Call(fun, args, ann) => Exp::Call(
                fun.clone(),
                args.iter().map(|e| e.map_ann(f)).collect(),
                f(ann),
            ),
        }
    }
}

impl<Ann> SeqExp<Ann> {
    pub fn ann(&self) -> Ann
    where
        Ann: Clone,
    {
        match self {
            SeqExp::Imm(_, a) => a.clone(),
            SeqExp::Prim1(_, _, a) => a.clone(),
            SeqExp::Prim2(_, _, _, a) => a.clone(),
            SeqExp::Let { ann: a, .. } => a.clone(),
            SeqExp::If { ann: a, .. } => a.clone(),
            SeqExp::Call(_, _, a) => a.clone(),
        }
    }

    pub fn map_ann<Ann2, F>(&self, f: &mut F) -> SeqExp<Ann2>
    where
        F: FnMut(&Ann) -> Ann2,
    {
        match self {
            SeqExp::Imm(imm, a) => SeqExp::Imm(imm.clone(), f(a)),
            SeqExp::Prim1(op, imm, a) => SeqExp::Prim1(*op, imm.clone(), f(a)),
            SeqExp::Prim2(op, imm1, imm2, a) => {
                SeqExp::Prim2(*op, imm1.clone(), imm2.clone(), f(a))
            }
            SeqExp::Let {
                var,
                bound_exp,
                body,
                ann,
            } => SeqExp::Let {
                var: var.clone(),
                bound_exp: Box::new(bound_exp.map_ann(f)),
                body: Box::new(body.map_ann(f)),
                ann: f(ann),
            },
            SeqExp::If {
                cond,
                thn,
                els,
                ann,
            } => SeqExp::If {
                cond: cond.clone(),
                thn: Box::new(thn.map_ann(f)),
                els: Box::new(els.map_ann(f)),
                ann: f(ann),
            },
            SeqExp::Call(fun, args, ann) => SeqExp::Call(fun.clone(), args.clone(), f(ann)),
        }
    }
}

impl<Ann> Prog<Exp<Ann>, Ann> {
    pub fn map_ann<Ann2, F>(&self, f: &mut F) -> Prog<Exp<Ann2>, Ann2>
    where
        F: FnMut(&Ann) -> Ann2,
    {
        Prog {
            funs: self.funs.iter().map(|d| d.map_ann(f)).collect(),
            main: self.main.map_ann(f),
            ann: f(&self.ann),
        }
    }
}

impl<Ann> FunDecl<Exp<Ann>, Ann> {
    pub fn map_ann<Ann2, F>(&self, f: &mut F) -> FunDecl<Exp<Ann2>, Ann2>
    where
        F: FnMut(&Ann) -> Ann2,
    {
        FunDecl {
            name: self.name.clone(),
            parameters: self.parameters.clone(),
            body: self.body.map_ann(f),
            ann: f(&self.ann),
        }
    }
}

impl<Ann> Prog<SeqExp<Ann>, Ann> {
    pub fn map_ann<Ann2, F>(&self, f: &mut F) -> Prog<SeqExp<Ann2>, Ann2>
    where
        F: FnMut(&Ann) -> Ann2,
    {
        Prog {
            funs: self.funs.iter().map(|d| d.map_ann(f)).collect(),
            main: self.main.map_ann(f),
            ann: f(&self.ann),
        }
    }
}

impl<AnnInner, AnnOuter> Prog<SeqExp<AnnInner>, AnnOuter> {
    pub fn map_ann_inner<AnnInner2, F>(&self, f: &mut F) -> Prog<SeqExp<AnnInner2>, AnnOuter>
    where
        F: FnMut(&AnnInner) -> AnnInner2,
        AnnOuter: Clone,
    {
        Prog {
            funs: self.funs.iter().map(|d| d.map_ann_inner(f)).collect(),
            main: self.main.map_ann(f),
            ann: self.ann.clone(),
        }
    }
}

impl<Ann> FunDecl<SeqExp<Ann>, Ann> {
    pub fn map_ann<Ann2, F>(&self, f: &mut F) -> FunDecl<SeqExp<Ann2>, Ann2>
    where
        F: FnMut(&Ann) -> Ann2,
    {
        FunDecl {
            name: self.name.clone(),
            parameters: self.parameters.clone(),
            body: self.body.map_ann(f),
            ann: f(&self.ann),
        }
    }
}

impl<AnnInner, AnnOuter> FunDecl<SeqExp<AnnInner>, AnnOuter> {
    pub fn map_ann_inner<AnnInner2, F>(&self, f: &mut F) -> FunDecl<SeqExp<AnnInner2>, AnnOuter>
    where
        F: FnMut(&AnnInner) -> AnnInner2,
        AnnOuter: Clone,
    {
        FunDecl {
            name: self.name.clone(),
            parameters: self.parameters.clone(),
            body: self.body.map_ann(f),
            ann: self.ann.clone(),
        }
    }
}

/* Tagging, unique names etc */

pub fn tag_prog<Ann>(p: &SurfProg<Ann>) -> SurfProg<u32> {
    let mut i = 0;
    p.map_ann(
        &mut (|_| {
            let cur = i;
            i += 1;
            cur
        }),
    )
}

pub fn tag_sprog<Ann>(p: &SeqProg<Ann>) -> SeqProg<u32> {
    let mut i = 0;
    p.map_ann(
        &mut (|_| {
            let cur = i;
            i += 1;
            cur
        }),
    )
}

pub fn add_tag_sprog<Ann1, Ann2>(p: &Prog<SeqExp<Ann1>, Ann2>) -> Prog<SeqExp<(Ann1, u32)>, Ann2>
where
    Ann1: Clone,
    Ann2: Clone,
{
    let mut i = 0;
    p.map_ann_inner(
        &mut (|ann| {
            let cur = i;
            i += 1;
            (ann.clone(), cur)
        }),
    )
}

use crate::list::List;

fn uniquify_names_imm(i: &ImmExp, env: List<(&str, String)>) -> ImmExp {
    match i {
        ImmExp::Var(s) => match env.lookup(s) {
            None => ImmExp::Var(s.clone()),
            Some(new_name) => ImmExp::Var(new_name),
        },
        _ => i.clone(),
    }
}

fn uniquify_names_exp<'e>(e: &'e SeqExp<u32>, env: List<(&'e str, String)>) -> SeqExp<u32> {
    match e {
        SeqExp::Imm(i, ann) => SeqExp::Imm(uniquify_names_imm(i, env), *ann),
        SeqExp::If {
            cond,
            thn,
            els,
            ann,
        } => SeqExp::If {
            cond: uniquify_names_imm(cond, env.clone()),
            thn: Box::new(uniquify_names_exp(thn, env.clone())),
            els: Box::new(uniquify_names_exp(els, env)),
            ann: *ann,
        },

        SeqExp::Let {
            var,
            bound_exp,
            body,
            ann,
        } => {
            let new_name = format!("{}_{}", var, ann);
            let bound_exp = Box::new(uniquify_names_exp(bound_exp, env.clone()));
            SeqExp::Let {
                var: new_name.clone(),
                bound_exp,
                body: Box::new(uniquify_names_exp(body, env.cons((var, new_name)))),
                ann: *ann,
            }
        }
        SeqExp::Call(f, args, ann) => SeqExp::Call(
            f.clone(),
            args.iter()
                .map(|i| uniquify_names_imm(i, env.clone()))
                .collect(),
            *ann,
        ),
        SeqExp::Prim1(op, i, ann) => SeqExp::Prim1(*op, uniquify_names_imm(i, env), *ann),
        SeqExp::Prim2(op, i1, i2, ann) => SeqExp::Prim2(
            *op,
            uniquify_names_imm(i1, env.clone()),
            uniquify_names_imm(i2, env),
            *ann,
        ),
    }
}

pub fn uniquify_names(p: &SeqProg<u32>) -> SeqProg<u32> {
    SeqProg {
        funs: p
            .funs
            .iter()
            .map(|f| FunDecl {
                name: f.name.clone(),
                parameters: f.parameters.clone(),
                body: uniquify_names_exp(&f.body, List::empty()),
                ann: f.ann,
            })
            .collect(),
        main: uniquify_names_exp(&p.main, List::empty()),
        ann: p.ann,
    }
}
