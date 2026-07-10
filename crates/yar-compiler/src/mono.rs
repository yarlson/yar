use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::*,
    diag::{Diagnostic, List},
};

pub fn monomorphize_program(program: &Program) -> (Program, Vec<Diagnostic>) {
    let mut mono = Monomorphizer::new(program);
    mono.index(program);

    for decl in &program.interfaces {
        let rewritten = mono.rewrite_interface(decl, None);
        mono.output.interfaces.push(rewritten);
    }
    for decl in &program.structs {
        if decl.type_params.is_empty() {
            let rewritten = mono.rewrite_struct(decl, None);
            mono.output.structs.push(rewritten);
        }
    }
    for decl in &program.functions {
        if decl.type_params.is_empty() {
            let rewritten = mono.rewrite_function(decl, None);
            mono.output.functions.push(rewritten);
        }
    }

    (mono.output, mono.diag.items())
}

struct Monomorphizer {
    diag: List,
    generic_structs: BTreeMap<String, StructDecl>,
    generic_functions: BTreeMap<String, FunctionDecl>,
    non_generic_functions: BTreeSet<String>,
    non_generic_named_types: BTreeSet<String>,
    struct_instantiating: BTreeSet<String>,
    function_instantiating: BTreeSet<String>,
    output: Program,
}

impl Monomorphizer {
    fn new(program: &Program) -> Self {
        Self {
            diag: List::default(),
            generic_structs: BTreeMap::new(),
            generic_functions: BTreeMap::new(),
            non_generic_functions: BTreeSet::new(),
            non_generic_named_types: BTreeSet::new(),
            struct_instantiating: BTreeSet::new(),
            function_instantiating: BTreeSet::new(),
            output: Program {
                package_pos: program.package_pos.clone(),
                package_name: program.package_name.clone(),
                imports: program.imports.clone(),
                enums: program.enums.clone(),
                ..Program::default()
            },
        }
    }

    fn index(&mut self, program: &Program) {
        for decl in &program.enums {
            self.non_generic_named_types.insert(decl.name.clone());
        }
        for decl in &program.interfaces {
            self.non_generic_named_types.insert(decl.name.clone());
        }
        for decl in &program.structs {
            if decl.type_params.is_empty() {
                self.non_generic_named_types.insert(decl.name.clone());
            } else {
                self.generic_structs.insert(decl.name.clone(), decl.clone());
            }
        }
        for decl in &program.functions {
            if decl.type_params.is_empty() {
                self.non_generic_functions.insert(decl.name.clone());
            } else {
                self.generic_functions
                    .insert(decl.name.clone(), decl.clone());
            }
        }
    }

    fn rewrite_struct(
        &mut self,
        decl: &StructDecl,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> StructDecl {
        StructDecl {
            struct_pos: decl.struct_pos.clone(),
            exported: decl.exported,
            name: decl.name.clone(),
            name_pos: decl.name_pos.clone(),
            type_params: Vec::new(),
            fields: decl
                .fields
                .iter()
                .map(|field| StructField {
                    name: field.name.clone(),
                    name_pos: field.name_pos.clone(),
                    type_ref: self.rewrite_type_ref(&field.type_ref, subst),
                })
                .collect(),
        }
    }

    fn rewrite_interface(
        &mut self,
        decl: &InterfaceDecl,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> InterfaceDecl {
        InterfaceDecl {
            interface_pos: decl.interface_pos.clone(),
            exported: decl.exported,
            name: decl.name.clone(),
            name_pos: decl.name_pos.clone(),
            methods: decl
                .methods
                .iter()
                .map(|method| InterfaceMethodDecl {
                    name: method.name.clone(),
                    name_pos: method.name_pos.clone(),
                    params: self.rewrite_params(&method.params, subst),
                    return_type: self.rewrite_type_ref(&method.return_type, subst),
                    return_is_bang: method.return_is_bang,
                })
                .collect(),
        }
    }

    fn rewrite_function(
        &mut self,
        decl: &FunctionDecl,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> FunctionDecl {
        FunctionDecl {
            exported: decl.exported,
            name: decl.name.clone(),
            name_pos: decl.name_pos.clone(),
            type_params: Vec::new(),
            receiver: decl.receiver.as_ref().map(|receiver| ReceiverDecl {
                name: receiver.name.clone(),
                name_pos: receiver.name_pos.clone(),
                type_ref: self.rewrite_type_ref(&receiver.type_ref, subst),
            }),
            params: self.rewrite_params(&decl.params, subst),
            return_type: self.rewrite_type_ref(&decl.return_type, subst),
            return_is_bang: decl.return_is_bang,
            body: self.rewrite_block(&decl.body, subst),
        }
    }

    fn rewrite_params(
        &mut self,
        params: &[Param],
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> Vec<Param> {
        params
            .iter()
            .map(|param| Param {
                name: param.name.clone(),
                name_pos: param.name_pos.clone(),
                type_ref: self.rewrite_type_ref(&param.type_ref, subst),
            })
            .collect()
    }

    fn rewrite_block(
        &mut self,
        block: &BlockStmt,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> BlockStmt {
        BlockStmt {
            lbrace: block.lbrace.clone(),
            stmts: block
                .stmts
                .iter()
                .map(|stmt| self.rewrite_statement(stmt, subst))
                .collect(),
        }
    }

    fn rewrite_statement(
        &mut self,
        stmt: &Statement,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> Statement {
        match stmt {
            Statement::Block(block) => Statement::Block(Box::new(self.rewrite_block(block, subst))),
            Statement::Let(stmt) => Statement::Let(Box::new(LetStmt {
                let_pos: stmt.let_pos.clone(),
                name: stmt.name.clone(),
                name_pos: stmt.name_pos.clone(),
                value: self.rewrite_expr(&stmt.value, subst),
            })),
            Statement::Var(stmt) => Statement::Var(Box::new(VarStmt {
                var_pos: stmt.var_pos.clone(),
                name: stmt.name.clone(),
                name_pos: stmt.name_pos.clone(),
                type_ref: self.rewrite_type_ref(&stmt.type_ref, subst),
                value: stmt
                    .value
                    .as_ref()
                    .map(|expr| self.rewrite_expr(expr, subst)),
            })),
            Statement::Assign(stmt) => Statement::Assign(Box::new(AssignStmt {
                target: self.rewrite_expr(&stmt.target, subst),
                value: self.rewrite_expr(&stmt.value, subst),
            })),
            Statement::CompoundAssign(stmt) => {
                Statement::CompoundAssign(Box::new(CompoundAssignStmt {
                    target: self.rewrite_expr(&stmt.target, subst),
                    operator: stmt.operator,
                    op_pos: stmt.op_pos.clone(),
                    value: self.rewrite_expr(&stmt.value, subst),
                }))
            }
            Statement::If(stmt) => Statement::If(Box::new(IfStmt {
                if_pos: stmt.if_pos.clone(),
                cond: self.rewrite_expr(&stmt.cond, subst),
                then_block: self.rewrite_block(&stmt.then_block, subst),
                else_stmt: stmt
                    .else_stmt
                    .as_ref()
                    .map(|else_stmt| self.rewrite_statement(else_stmt, subst)),
            })),
            Statement::For(stmt) => Statement::For(Box::new(ForStmt {
                for_pos: stmt.for_pos.clone(),
                init: stmt
                    .init
                    .as_ref()
                    .map(|stmt| self.rewrite_statement(stmt, subst)),
                cond: stmt
                    .cond
                    .as_ref()
                    .map(|expr| self.rewrite_expr(expr, subst)),
                post: stmt
                    .post
                    .as_ref()
                    .map(|stmt| self.rewrite_statement(stmt, subst)),
                body: self.rewrite_block(&stmt.body, subst),
            })),
            Statement::Break(stmt) => Statement::Break(stmt.clone()),
            Statement::Continue(stmt) => Statement::Continue(stmt.clone()),
            Statement::Return(stmt) => Statement::Return(Box::new(ReturnStmt {
                return_pos: stmt.return_pos.clone(),
                value: stmt
                    .value
                    .as_ref()
                    .map(|expr| self.rewrite_expr(expr, subst)),
            })),
            Statement::Match(stmt) => Statement::Match(Box::new(MatchStmt {
                match_pos: stmt.match_pos.clone(),
                value: self.rewrite_expr(&stmt.value, subst),
                arms: stmt
                    .arms
                    .iter()
                    .map(|arm| MatchArm {
                        case_pos: arm.case_pos.clone(),
                        enum_type: self.rewrite_type_ref(&arm.enum_type, subst),
                        case_name: arm.case_name.clone(),
                        case_name_pos: arm.case_name_pos.clone(),
                        bind_name: arm.bind_name.clone(),
                        bind_name_pos: arm.bind_name_pos.clone(),
                        bind_ignore: arm.bind_ignore,
                        body: self.rewrite_block(&arm.body, subst),
                    })
                    .collect(),
                else_body: stmt
                    .else_body
                    .as_ref()
                    .map(|block| self.rewrite_block(block, subst)),
            })),
            Statement::Expr(stmt) => Statement::Expr(Box::new(ExprStmt {
                expr: self.rewrite_expr(&stmt.expr, subst),
            })),
            Statement::Spawn(stmt) => Statement::Spawn(Box::new(SpawnStmt {
                spawn_pos: stmt.spawn_pos.clone(),
                call: self.rewrite_expr(&stmt.call, subst),
            })),
        }
    }

    fn rewrite_expr(
        &mut self,
        expr: &Expression,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> Expression {
        match expr {
            Expression::Ident(expr) => Expression::Ident(expr.clone()),
            Expression::Int(expr) => Expression::Int(expr.clone()),
            Expression::Char(expr) => Expression::Char(expr.clone()),
            Expression::String(expr) => Expression::String(expr.clone()),
            Expression::Bool(expr) => Expression::Bool(expr.clone()),
            Expression::Nil(expr) => Expression::Nil(expr.clone()),
            Expression::Error(expr) => Expression::Error(expr.clone()),
            Expression::Group(expr) => Expression::Group(Box::new(GroupExpr {
                inner: self.rewrite_expr(&expr.inner, subst),
            })),
            Expression::FunctionLiteral(expr) => {
                Expression::FunctionLiteral(Box::new(FunctionLiteralExpr {
                    fn_pos: expr.fn_pos.clone(),
                    params: self.rewrite_params(&expr.params, subst),
                    return_type: self.rewrite_type_ref(&expr.return_type, subst),
                    return_is_bang: expr.return_is_bang,
                    body: self.rewrite_block(&expr.body, subst),
                }))
            }
            Expression::Taskgroup(expr) => Expression::Taskgroup(Box::new(TaskgroupExpr {
                taskgroup_pos: expr.taskgroup_pos.clone(),
                result_type: self.rewrite_type_ref(&expr.result_type, subst),
                body: self.rewrite_block(&expr.body, subst),
            })),
            Expression::Unary(expr) => Expression::Unary(Box::new(UnaryExpr {
                operator: expr.operator,
                op_pos: expr.op_pos.clone(),
                inner: self.rewrite_expr(&expr.inner, subst),
            })),
            Expression::Binary(expr) => Expression::Binary(Box::new(BinaryExpr {
                left: self.rewrite_expr(&expr.left, subst),
                operator: expr.operator,
                op_pos: expr.op_pos.clone(),
                right: self.rewrite_expr(&expr.right, subst),
            })),
            Expression::Selector(expr) => Expression::Selector(Box::new(SelectorExpr {
                inner: self.rewrite_expr(&expr.inner, subst),
                dot_pos: expr.dot_pos.clone(),
                name: expr.name.clone(),
                name_pos: expr.name_pos.clone(),
            })),
            Expression::Index(expr) => Expression::Index(Box::new(IndexExpr {
                inner: self.rewrite_expr(&expr.inner, subst),
                lbracket_pos: expr.lbracket_pos.clone(),
                index: self.rewrite_expr(&expr.index, subst),
            })),
            Expression::Slice(expr) => Expression::Slice(Box::new(SliceExpr {
                inner: self.rewrite_expr(&expr.inner, subst),
                lbracket_pos: expr.lbracket_pos.clone(),
                start: expr
                    .start
                    .as_ref()
                    .map(|expr| self.rewrite_expr(expr, subst)),
                colon_pos: expr.colon_pos.clone(),
                end: expr.end.as_ref().map(|expr| self.rewrite_expr(expr, subst)),
            })),
            Expression::Call(expr) => self.rewrite_call(expr, subst),
            Expression::TypeApplication(expr) => {
                self.diag.add(
                    expr.lbracket_pos.clone(),
                    "type arguments are only supported on generic function calls",
                );
                self.rewrite_expr(&expr.inner, subst)
            }
            Expression::StructLiteral(expr) => {
                Expression::StructLiteral(Box::new(StructLiteralExpr {
                    type_ref: self.rewrite_type_ref(&expr.type_ref, subst),
                    lbrace: expr.lbrace.clone(),
                    fields: expr
                        .fields
                        .iter()
                        .map(|field| StructLiteralField {
                            name: field.name.clone(),
                            name_pos: field.name_pos.clone(),
                            value: self.rewrite_expr(&field.value, subst),
                        })
                        .collect(),
                }))
            }
            Expression::ArrayLiteral(expr) => {
                Expression::ArrayLiteral(Box::new(ArrayLiteralExpr {
                    type_ref: self.rewrite_type_ref(&expr.type_ref, subst),
                    lbrace: expr.lbrace.clone(),
                    elements: expr
                        .elements
                        .iter()
                        .map(|element| self.rewrite_expr(element, subst))
                        .collect(),
                }))
            }
            Expression::SliceLiteral(expr) => {
                Expression::SliceLiteral(Box::new(SliceLiteralExpr {
                    type_ref: self.rewrite_type_ref(&expr.type_ref, subst),
                    lbrace: expr.lbrace.clone(),
                    elements: expr
                        .elements
                        .iter()
                        .map(|element| self.rewrite_expr(element, subst))
                        .collect(),
                }))
            }
            Expression::MapLiteral(expr) => Expression::MapLiteral(Box::new(MapLiteralExpr {
                type_ref: self.rewrite_type_ref(&expr.type_ref, subst),
                lbrace: expr.lbrace.clone(),
                pairs: expr
                    .pairs
                    .iter()
                    .map(|pair| MapLiteralPair {
                        key: self.rewrite_expr(&pair.key, subst),
                        key_pos: pair.key_pos.clone(),
                        value: self.rewrite_expr(&pair.value, subst),
                        value_pos: pair.value_pos.clone(),
                    })
                    .collect(),
            })),
            Expression::Propagate(expr) => Expression::Propagate(Box::new(PropagateExpr {
                inner: self.rewrite_expr(&expr.inner, subst),
                question_pos: expr.question_pos.clone(),
            })),
            Expression::Handle(expr) => Expression::Handle(Box::new(HandleExpr {
                inner: self.rewrite_expr(&expr.inner, subst),
                or_pos: expr.or_pos.clone(),
                err_name: expr.err_name.clone(),
                err_pos: expr.err_pos.clone(),
                handler: self.rewrite_block(&expr.handler, subst),
            })),
            Expression::Missing(pos) => Expression::Missing(pos.clone()),
        }
    }

    fn rewrite_call(
        &mut self,
        call: &CallExpr,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> Expression {
        let args = call
            .args
            .iter()
            .map(|arg| self.rewrite_expr(arg, subst))
            .collect::<Vec<_>>();

        if let Expression::TypeApplication(applied) = &call.callee {
            let Expression::Ident(ident) = &applied.inner else {
                self.diag.add(
                    applied.lbracket_pos.clone(),
                    "type arguments are only supported on named functions",
                );
                return Expression::Call(Box::new(CallExpr {
                    callee: self.rewrite_expr(&applied.inner, subst),
                    args,
                }));
            };

            let type_args = applied
                .type_args
                .iter()
                .map(|arg| self.rewrite_type_ref(arg, subst))
                .collect::<Vec<_>>();

            if let Some(decl) = self.generic_functions.get(&ident.name).cloned() {
                if type_args.len() != decl.type_params.len() {
                    self.diag.add(
                        applied.lbracket_pos.clone(),
                        format!(
                            "generic function {:?} expects {} type arguments, got {}",
                            diagnostic_name(&ident.name),
                            decl.type_params.len(),
                            type_args.len()
                        ),
                    );
                    return Expression::Call(Box::new(CallExpr {
                        callee: Expression::Ident(ident.clone()),
                        args,
                    }));
                }
                let name = self.instantiate_function(&decl, &type_args);
                return Expression::Call(Box::new(CallExpr {
                    callee: Expression::Ident(Box::new(IdentExpr {
                        name,
                        name_pos: ident.name_pos.clone(),
                    })),
                    args,
                }));
            }

            if ident.name == "chan_new" {
                return Expression::Call(Box::new(CallExpr {
                    callee: Expression::TypeApplication(Box::new(TypeApplicationExpr {
                        inner: Expression::Ident(ident.clone()),
                        lbracket_pos: applied.lbracket_pos.clone(),
                        type_args,
                    })),
                    args,
                }));
            }

            if self.non_generic_functions.contains(&ident.name) || is_builtin_function(&ident.name)
            {
                self.diag.add(
                    applied.lbracket_pos.clone(),
                    format!("function {:?} is not generic", diagnostic_name(&ident.name)),
                );
            } else {
                self.diag.add(
                    ident.name_pos.clone(),
                    format!("unknown function {:?}", diagnostic_name(&ident.name)),
                );
            }
            return Expression::Call(Box::new(CallExpr {
                callee: Expression::Ident(ident.clone()),
                args,
            }));
        }

        if let Expression::Ident(ident) = &call.callee
            && self.generic_functions.contains_key(&ident.name)
        {
            self.diag.add(
                ident.name_pos.clone(),
                format!(
                    "generic function {:?} requires explicit type arguments",
                    diagnostic_name(&ident.name)
                ),
            );
        }

        Expression::Call(Box::new(CallExpr {
            callee: self.rewrite_expr(&call.callee, subst),
            args,
        }))
    }

    fn rewrite_type_ref(
        &mut self,
        ref_: &TypeRef,
        subst: Option<&BTreeMap<String, TypeRef>>,
    ) -> TypeRef {
        let mut out = ref_.clone();
        match ref_.kind {
            TypeRefKind::Errorable
            | TypeRefKind::Pointer
            | TypeRefKind::Array
            | TypeRefKind::Slice
            | TypeRefKind::Chan => {
                out.elem = ref_
                    .elem
                    .as_deref()
                    .map(|elem| Box::new(self.rewrite_type_ref(elem, subst)));
                return out;
            }
            TypeRefKind::Map => {
                out.key = ref_
                    .key
                    .as_deref()
                    .map(|key| Box::new(self.rewrite_type_ref(key, subst)));
                out.value = ref_
                    .value
                    .as_deref()
                    .map(|value| Box::new(self.rewrite_type_ref(value, subst)));
                return out;
            }
            TypeRefKind::Function => {
                out.params = ref_
                    .params
                    .iter()
                    .map(|param| self.rewrite_type_ref(param, subst))
                    .collect();
                out.return_type = ref_
                    .return_type
                    .as_deref()
                    .map(|ret| Box::new(self.rewrite_type_ref(ret, subst)));
                return out;
            }
            TypeRefKind::Named => {}
        }

        if let Some(replacement) = subst.and_then(|subst| subst.get(&ref_.name)) {
            if !ref_.type_args.is_empty() {
                self.diag.add(
                    ref_.pos.clone(),
                    format!("type parameter {:?} cannot take type arguments", ref_.name),
                );
            }
            return replacement.clone();
        }

        if ref_.type_args.is_empty() {
            if self.generic_structs.contains_key(&ref_.name) {
                self.diag.add(
                    ref_.pos.clone(),
                    format!(
                        "generic type {:?} requires explicit type arguments",
                        diagnostic_name(&ref_.name)
                    ),
                );
            }
            return TypeRef::named(ref_.name.clone(), ref_.pos.clone());
        }

        let type_args = ref_
            .type_args
            .iter()
            .map(|arg| self.rewrite_type_ref(arg, subst))
            .collect::<Vec<_>>();

        if let Some(decl) = self.generic_structs.get(&ref_.name).cloned() {
            if type_args.len() != decl.type_params.len() {
                self.diag.add(
                    ref_.pos.clone(),
                    format!(
                        "generic type {:?} expects {} type arguments, got {}",
                        diagnostic_name(&ref_.name),
                        decl.type_params.len(),
                        type_args.len()
                    ),
                );
                return TypeRef::named(ref_.name.clone(), ref_.pos.clone());
            }
            return TypeRef::named(self.instantiate_struct(&decl, &type_args), ref_.pos.clone());
        }

        if is_builtin_type(&ref_.name) || self.non_generic_named_types.contains(&ref_.name) {
            self.diag.add(
                ref_.pos.clone(),
                format!("type {:?} is not generic", diagnostic_name(&ref_.name)),
            );
            return TypeRef::named(ref_.name.clone(), ref_.pos.clone());
        }

        self.diag.add(
            ref_.pos.clone(),
            format!("unknown type {:?}", ref_.to_string()),
        );
        TypeRef::named(ref_.name.clone(), ref_.pos.clone())
    }

    fn instantiate_struct(&mut self, decl: &StructDecl, type_args: &[TypeRef]) -> String {
        let name = instantiated_name(&decl.name, type_args);
        if self.struct_instantiating.contains(&name)
            || self
                .output
                .structs
                .iter()
                .any(|existing| existing.name == name)
        {
            return name;
        }

        self.struct_instantiating.insert(name.clone());
        let subst = make_type_substitution(&decl.type_params, type_args);
        let mut instantiated = self.rewrite_struct(decl, Some(&subst));
        instantiated.name = name.clone();
        self.output.structs.push(instantiated);
        self.struct_instantiating.remove(&name);
        self.non_generic_named_types.insert(name.clone());
        name
    }

    fn instantiate_function(&mut self, decl: &FunctionDecl, type_args: &[TypeRef]) -> String {
        let name = instantiated_name(&decl.name, type_args);
        if self.function_instantiating.contains(&name)
            || self
                .output
                .functions
                .iter()
                .any(|existing| existing.name == name)
        {
            return name;
        }

        self.function_instantiating.insert(name.clone());
        let subst = make_type_substitution(&decl.type_params, type_args);
        let mut instantiated = self.rewrite_function(decl, Some(&subst));
        instantiated.name = name.clone();
        self.output.functions.push(instantiated);
        self.function_instantiating.remove(&name);
        self.non_generic_functions.insert(name.clone());
        name
    }
}

fn make_type_substitution(params: &[TypeParam], args: &[TypeRef]) -> BTreeMap<String, TypeRef> {
    params
        .iter()
        .zip(args)
        .map(|(param, arg)| (param.name.clone(), arg.clone()))
        .collect()
}

fn instantiated_name(base: &str, args: &[TypeRef]) -> String {
    let args = args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("{base}[{args}]")
}

fn diagnostic_name(name: &str) -> &str {
    name.strip_prefix("main.").unwrap_or(name)
}

fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "void" | "noreturn" | "bool" | "i32" | "i64" | "str" | "error"
    )
}

fn is_builtin_function(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "panic"
            | "len"
            | "append"
            | "has"
            | "delete"
            | "keys"
            | "to_str"
            | "sb_new"
            | "sb_write"
            | "sb_string"
            | "chan_new"
            | "chan_send"
            | "chan_recv"
            | "chan_close"
            | "chr"
            | "i32_to_i64"
            | "i64_to_i32"
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::{lower::lower_package_graph, package::load_package_graph};

    use super::*;

    #[test]
    fn monomorphizes_local_generic_fixture() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/generics/main.yar"), false).unwrap();
        assert_eq!(diagnostics, Vec::new());
        let (lowered, diagnostics) = lower_package_graph(&graph);
        assert_eq!(diagnostics, Vec::new());

        let (program, diagnostics) = monomorphize_program(&lowered);
        assert_eq!(diagnostics, Vec::new());

        assert!(
            program
                .structs
                .iter()
                .any(|decl| decl.name == "main.Box[i32]")
        );
        assert!(
            program
                .structs
                .iter()
                .any(|decl| decl.name == "main.Pair[str,i32]")
        );
        assert!(
            program
                .functions
                .iter()
                .any(|decl| decl.name == "main.first[i32]")
        );
        assert!(
            program
                .functions
                .iter()
                .any(|decl| decl.name == "main.wrap[i32]")
        );
        assert!(
            !program
                .functions
                .iter()
                .any(|decl| !decl.type_params.is_empty())
        );
    }

    #[test]
    fn monomorphizes_imported_generic_fixture() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/generics_imports/main.yar"), false).unwrap();
        assert_eq!(diagnostics, Vec::new());
        let (lowered, diagnostics) = lower_package_graph(&graph);
        assert_eq!(diagnostics, Vec::new());

        let (program, diagnostics) = monomorphize_program(&lowered);
        assert_eq!(diagnostics, Vec::new());

        assert!(
            program
                .structs
                .iter()
                .any(|decl| decl.name == "lib.Box[str]")
        );
        assert!(
            program
                .functions
                .iter()
                .any(|decl| decl.name == "lib.wrap[str]")
        );
    }

    #[test]
    fn monomorphizes_every_testdata_entry_without_diagnostics() {
        let root = repo_root();
        let mut entries = Vec::new();
        collect_main_files(&root.join("testdata"), &mut entries);
        entries.sort();

        let mut failures = Vec::new();
        for entry in entries {
            let (graph, load_diagnostics) =
                load_package_graph(&entry, false).unwrap_or_else(|err| {
                    panic!("load {}: {err}", entry.display());
                });
            if !load_diagnostics.is_empty() {
                failures.push(format!("{} load: {:?}", entry.display(), load_diagnostics));
                continue;
            }
            let (lowered, lower_diagnostics) = lower_package_graph(&graph);
            if !lower_diagnostics.is_empty() {
                failures.push(format!(
                    "{} lower: {:?}",
                    entry.display(),
                    lower_diagnostics
                ));
                continue;
            }
            let (program, mono_diagnostics) = monomorphize_program(&lowered);
            if !mono_diagnostics.is_empty() {
                failures.push(format!("{} mono: {:?}", entry.display(), mono_diagnostics));
                continue;
            }
            if !program
                .functions
                .iter()
                .any(|function| function.name == "main")
            {
                failures.push(format!("{}: missing monomorphized main", entry.display()));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate is nested under crates/yar-compiler")
            .to_path_buf()
    }

    fn collect_main_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in
            std::fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        {
            let path = entry
                .unwrap_or_else(|err| panic!("read entry in {}: {err}", dir.display()))
                .path();
            if path.is_dir() {
                collect_main_files(&path, out);
                continue;
            }
            if path.file_name().is_some_and(|name| name == "main.yar") {
                out.push(path);
            }
        }
    }
}
