use std::ops::Deref;

use crate::{
    ast::{
        node::{self, Ident},
        Decl, Expr, NodeSpan, Stmt, Type,
    },
    reporting::error::PreciseError,
    token::{Span, Token, TokenKind},
    Source,
};

use super::symbol::{Binding, SymbolTable};

macro_rules! symbol {
    ($self:ident, $name:expr, $ty:ident, $s:literal) => {
        match $self.symbol(&$name) {
            Some(Binding::$ty(v)) => v.clone(),
            Some(_) => {
                $self.error(
                    $name.span,
                    format!("`{}` is not a {}", $name, $s),
                    "".into(),
                );
                return Err(TypeError::NotType($name.clone(), $s));
            }
            None => {
                let name = String::from(&$name);
                if name != "println" && name != "max" && name == "min" {
                    $self.error($name.span, format!("`{}` is not defined", $name), "".into());
                    return Err(TypeError::Undefined);
                }
                return Ok(Type::Void);
            }
        }
    };
}

macro_rules! cast {
    ($id:expr, $res:expr, $pattern:pat) => {
        match $id {
            $pattern => $res,
            _ => unimplemented!(),
        }
    };
}

#[derive(thiserror::Error, Debug)]
pub enum TypeError {
    #[error("undefined variable")]
    Undefined,
    #[error("`{0}` is not a {1}")]
    NotType(Token, &'static str),
    #[error("cannot {0} {1}")]
    UnaryMismatch(&'static str, Type),
    #[error("expected {0}, got {1}")]
    Mismatch(Type, Type),
    #[error("{0} is not a property of {1}")]
    NotProperty(Token, Type),
}

pub struct TypeCheckPass<'a> {
    source: &'a Source,
    program: &'a Vec<Decl>,
    errors: Vec<PreciseError>,
    function: Option<Token>,
    scopes: Vec<SymbolTable>,
}

trait Check {
    fn check(&self, pass: &mut TypeCheckPass<'_>) -> Result<Type, TypeError>;
}

impl Check for Decl {
    fn check(&self, pass: &mut TypeCheckPass<'_>) -> Result<Type, TypeError> {
        match self {
            Decl::Function(fun) => pass.function(fun),
            Decl::Constant(c) => pass.constant(c),
            Decl::Record(_) => Ok(Type::Void),
        }
    }
}

impl Check for Stmt {
    fn check(&self, pass: &mut TypeCheckPass<'_>) -> Result<Type, TypeError> {
        match self {
            Stmt::Return(r) => pass.ret(r),
            Stmt::Expr(e) => e.check(pass),
            Stmt::Var(v) => pass.var(v),
            Stmt::Assign(a) => pass.assign(a),
        }
    }
}

impl Check for Expr {
    fn check(&self, pass: &mut TypeCheckPass<'_>) -> Result<Type, TypeError> {
        match self {
            Expr::Binary(b) => pass.binary(b),
            Expr::Unary(u) => pass.unary(u),
            Expr::Call(c) => pass.call(c),
            Expr::Ident(i) => pass.ident(i),
            Expr::Access(access) => pass.access(access),
            Expr::Init(init) => pass.init(init),
            Expr::Bool(..) => Ok(Type::Bool),
            Expr::Int(..) => Ok(Type::Int),
            Expr::Float(..) => Ok(Type::Float),
            Expr::Str(..) => Ok(Type::Str),
        }
    }
}

impl<'a> TypeCheckPass<'a> {
    pub fn new(table: SymbolTable, source: &'a Source, program: &'a Vec<Decl>) -> Self {
        Self {
            source,
            program,
            errors: vec![],
            function: None,
            scopes: vec![table],
        }
    }

    pub fn run(&mut self) -> Result<(), usize> {
        for node in self.program {
            let _ = node.check(self);
        }
        let len = self.errors.len();
        if len > 0 {
            Err(len)
        } else {
            Ok(())
        }
    }

    fn scope_mut(&mut self) -> &mut SymbolTable {
        self.scopes.last_mut().unwrap()
    }

    fn begin_scope(&mut self) {
        self.scopes.push(SymbolTable::new());
    }

    fn end_scope(&mut self) {
        self.scopes.pop();
    }

    fn symbol(&self, name: &Token) -> Option<&Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(definition) = scope.get(name) {
                return Some(definition);
            }
        }
        None
    }

    fn error(&mut self, at: Span, heading: String, text: String) {
        let error = PreciseError::new(self.source, at, heading, text);
        println!("{}", error);
        self.errors.push(error);
    }

    fn unary(&mut self, unary: &node::Unary) -> Result<Type, TypeError> {
        let got = unary.right.check(self)?;
        match unary.op.kind {
            TokenKind::Minus => {
                if !matches!(got, Type::Int | Type::Float) {
                    self.error(
                        unary.right.span(),
                        format!("cannot negate {}", got),
                        format!("expression of type {}", got),
                    );
                    return Err(TypeError::UnaryMismatch("negate", got));
                }
                Ok(got)
            }
            TokenKind::Bang => {
                if got != Type::Bool {
                    self.error(
                        unary.right.span(),
                        format!("cannot invert {}", got),
                        format!("expression of type {}", got),
                    );
                    return Err(TypeError::UnaryMismatch("invert", got));
                }
                Ok(Type::Bool)
            }
            _ => unimplemented!(),
        }
    }

    fn assign(&mut self, a: &node::Assign) -> Result<Type, TypeError> {
        let expected = a.target.check(self)?;
        let got = a.expr.check(self)?;
        if got != expected {
            self.error(
                a.expr.span(),
                format!("expected expression of type {}", expected),
                format!("expression of type {}", got),
            );
        }
        Ok(Type::Void)
    }

    fn init(&mut self, init: &node::Init) -> Result<Type, TypeError> {
        let rec = symbol!(self, init.name, Record, "record");
        for initializer in &init.initializers {
            let got = initializer.expr.check(self)?;
            let expected = match rec.fields.iter().find(|f| f.name == initializer.name) {
                Some(field) => Type::from(&field.ty),
                None => {
                    self.error(
                        initializer.name.span,
                        format!("`{}` is not a field of `{}`", initializer.name, init.name),
                        "".into(),
                    );
                    continue;
                }
            };
            if got != expected {
                self.error(
                    initializer.expr.span(),
                    format!("expected initializer to be of type {}", expected),
                    format!("expression of type {}", got),
                );
            }
        }
        Ok(Type::from(&init.name))
    }

    // TODO: make this prettier
    fn access(&mut self, access: &node::Access) -> Result<Type, TypeError> {
        fn err(pass: &mut TypeCheckPass<'_>, ident: &Ident, ty: Type) -> Result<Type, TypeError> {
            pass.error(
                ident.name.span,
                format!("`{}` is not a property of `{}`", ident.name, ty),
                "".into(),
            );
            Err(TypeError::NotProperty(ident.name.clone(), ty))
        }
        let mut ty = access.chain[0].check(self)?;
        let mut binding = match self.symbol(&Token::from(&ty)).cloned() {
            Some(binding) => binding,
            None => return Err(TypeError::Undefined),
        };
        for (i, pair) in access.chain.windows(2).enumerate() {
            let (left, right) = (&pair[0], &pair[1]);
            if i != 0 {
                // TODO: implement member functions
                let rec = cast!(binding, r, Binding::Record(ref r));
                let ident = cast!(left, i, Expr::Ident(i));
                let field = rec.fields.iter().find(|f| f.name == ident.name);
                if let Some(field) = field {
                    binding = self.symbol(&field.ty).cloned().unwrap()
                } else {
                    return err(self, ident, ty);
                }
            }
            // TODO: implement member functions (same as above)
            let rec = cast!(binding, r, Binding::Record(ref r));
            let ident = cast!(right, i, Expr::Ident(i));
            let field = rec.fields.iter().find(|f| f.name == ident.name);
            if let Some(field) = field {
                ty = Type::from(&field.ty);
            } else {
                return err(self, ident, ty);
            }
        }
        Ok(ty)
    }

    fn constant(&mut self, c: &node::ConstantDecl) -> Result<Type, TypeError> {
        let got = c.expr.check(self)?;
        let expected = Type::from(&c.ty);
        if got != expected {
            self.error(
                c.expr.span(),
                format!("expected initializer to be of type {}", expected),
                format!("expression of type {}", got),
            );
        }
        Ok(expected)
    }

    fn var(&mut self, v: &node::VarDecl) -> Result<Type, TypeError> {
        let got = v.expr.check(self)?;
        let expected = Type::from(&v.ty);
        if got != expected {
            self.error(
                v.expr.span(),
                format!("expected initializer to be of type {}", expected),
                format!("expression of type {}", got),
            );
        }
        self.scope_mut()
            .insert(v.name.clone(), Binding::Variable(v.clone()));
        Ok(expected)
    }

    fn function(&mut self, fun: &node::FuncDecl) -> Result<Type, TypeError> {
        if String::from(&fun.name) == "main" {
            if let Some(ty) = &fun.ty {
                if String::from(ty) != "void" {
                    self.error(
                        ty.span,
                        "main function must return void".into(),
                        "try changing or removing this type".into(),
                    );
                }
            }
        }
        self.begin_scope();
        self.function = Some(fun.name.clone());
        for param in &fun.params {
            self.scope_mut()
                .insert(param.name.clone(), Binding::Function(fun.clone()));
        }
        for node in &fun.body {
            let _ = node.check(self);
        }
        self.end_scope();
        self.function = None;
        Ok(Type::Void)
    }

    fn ret(&mut self, r: &node::Return) -> Result<Type, TypeError> {
        let got = r.expr.check(self)?;
        match &self.function {
            Some(function) => {
                let symbol = self.symbol(function).unwrap().clone();
                if got != symbol.ty() {
                    self.error(
                        r.expr.span(),
                        format!("expected return type to be {}", symbol.ty()),
                        format!("expression is of type {}", got),
                    );
                    return Err(TypeError::Mismatch(symbol.ty(), got));
                }
            }
            None => unimplemented!("disallowed by parser"),
        }
        Ok(got)
    }

    fn binary(&mut self, b: &node::Binary) -> Result<Type, TypeError> {
        let lhs = b.left.check(self)?;
        let rhs = b.right.check(self)?;
        if lhs != rhs {
            let heading = match b.op.kind {
                TokenKind::Plus => format!("cannot add {} to {}", lhs, rhs),
                TokenKind::Minus => format!("cannot subtract {} from {}", rhs, lhs),
                TokenKind::Star => format!("cannot multiply {} by {}", lhs, rhs),
                TokenKind::Slash => format!("cannot divide {} by {}", lhs, rhs),
                _ => format!("cannot compare {} and {}", lhs, rhs),
            };
            self.error(b.op.span, heading, "".into());
            return Err(TypeError::Mismatch(lhs, rhs));
        }
        if matches!(
            b.op.kind,
            TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash
        ) {
            Ok(lhs)
        } else {
            Ok(Type::Bool)
        }
    }

    fn ident(&mut self, id: &node::Ident) -> Result<Type, TypeError> {
        Ok(match self.symbol(&id.name) {
            Some(Binding::Function(f)) => {
                let param = f.params.iter().find(|p| p.name == id.name).unwrap();
                Type::from(&param.ty)
            }
            Some(Binding::Variable(v)) => Type::from(&v.ty),
            Some(Binding::Constant(c)) => Type::from(&c.ty),
            _ => {
                self.error(
                    id.name.span,
                    format!("`{}` is not defined", &id.name),
                    "".into(),
                );
                return Err(TypeError::Undefined);
            }
        })
    }

    fn call(&mut self, call: &node::Call) -> Result<Type, TypeError> {
        let name = cast!(call.left.deref(), ident.name.clone(), Expr::Ident(ident));
        let function = symbol!(self, name, Function, "function");
        let (arity, params, ty) = (
            function.params.len(),
            function.params.clone(),
            function.ty.clone(),
        );
        if arity != call.args.len() {
            self.error(
                call.left.span(),
                format!(
                    "this function takes {} arguments, but {} were provided",
                    arity,
                    call.args.len()
                ),
                format!("while calling {} here", name),
            );
        }
        for (i, arg) in call.args.iter().enumerate() {
            let got = arg.check(self)?;
            if i < params.len() {
                let expected = Type::from(&params[i].ty);
                if got != expected {
                    self.error(
                        arg.span(),
                        format!("expected argument of type {}, but found {}", expected, got),
                        format!("expression of type {}", got),
                    );
                }
            }
        }
        Ok(if let Some(ty) = ty {
            Type::from(&ty)
        } else {
            Type::Void
        })
    }
}

macro_rules! assert_typecheck {
    ($($path:expr => $name:ident),*) => {
        #[cfg(test)]
        mod tests {
            use crate::{SymbolTable, pass::typecheck::TypeCheckPass};

            $(
                #[test]
                fn $name() -> Result<(), Box<dyn std::error::Error>> {
                    let source = crate::Source::new($path)?;
                    let ast = crate::ast::Ast::from_source(source.clone())?;
                    let symbols = SymbolTable::from(&ast.nodes);
                    let mut pass = TypeCheckPass::new(symbols, &source, &ast.nodes);
                    let _ = pass.run();
                    insta::with_settings!({snapshot_path => "../../snapshots"}, {
                        insta::assert_yaml_snapshot!(pass.errors);
                    });

                    Ok(())
                }
            )*
        }
    };
}

assert_typecheck! {
    "test-cases/typecheck/varied.kya" => varied,
    "test-cases/typecheck/records.kya" => records
}
