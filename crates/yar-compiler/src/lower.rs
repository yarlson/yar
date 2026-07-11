use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::*,
    checker::is_builtin_function,
    diag::{Diagnostic, List},
    symbol::canonical_decl_name,
    token::Position,
};

pub fn lower_package_graph(graph: &PackageGraph) -> (Program, Vec<Diagnostic>) {
    let mut lowerer = PackageLowerer::new(graph);
    lowerer.index_packages();
    lowerer.validate_exported_api();

    let Some(entry) = graph.packages.get(&graph.entry) else {
        return (Program::default(), lowerer.diag.items());
    };

    let mut program = Program {
        package_pos: entry
            .files
            .first()
            .map(|file| file.package_pos.clone())
            .unwrap_or_default(),
        package_name: "main".to_owned(),
        ..Program::default()
    };

    let mut package_order = vec![graph.entry.clone()];
    package_order.extend(
        graph
            .packages
            .keys()
            .filter(|path| *path != &graph.entry)
            .cloned(),
    );

    for path in package_order {
        let Some(package) = graph.packages.get(&path) else {
            continue;
        };
        program.enums.extend(lowerer.lower_enums(package));
        program.interfaces.extend(lowerer.lower_interfaces(package));
        program.structs.extend(lowerer.lower_structs(package));
        program.functions.extend(lowerer.lower_functions(package));
    }

    (program, lowerer.diag.items())
}

struct PackageLowerer<'a> {
    graph: &'a PackageGraph,
    structs: BTreeMap<PackageId, BTreeMap<String, DeclVisibility>>,
    interfaces: BTreeMap<PackageId, BTreeMap<String, DeclVisibility>>,
    enums: BTreeMap<PackageId, BTreeMap<String, DeclVisibility>>,
    functions: BTreeMap<PackageId, BTreeMap<String, DeclVisibility>>,
    imports: BTreeMap<PackageId, BTreeMap<String, PackageId>>,
    diag: List,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DeclVisibility {
    exported: bool,
}

impl<'a> PackageLowerer<'a> {
    fn new(graph: &'a PackageGraph) -> Self {
        Self {
            graph,
            structs: BTreeMap::new(),
            interfaces: BTreeMap::new(),
            enums: BTreeMap::new(),
            functions: BTreeMap::new(),
            imports: BTreeMap::new(),
            diag: List::default(),
        }
    }

    fn index_packages(&mut self) {
        for (path, package) in &self.graph.packages {
            let mut structs = BTreeMap::new();
            for decl in &package.structs {
                if structs
                    .insert(
                        decl.name.clone(),
                        DeclVisibility {
                            exported: decl.exported,
                        },
                    )
                    .is_some()
                {
                    self.diag.add(
                        decl.name_pos.clone(),
                        format!("struct {:?} is already declared", decl.name),
                    );
                }
            }
            self.structs.insert(path.clone(), structs);

            let mut interfaces = BTreeMap::new();
            for decl in &package.interfaces {
                if interfaces
                    .insert(
                        decl.name.clone(),
                        DeclVisibility {
                            exported: decl.exported,
                        },
                    )
                    .is_some()
                {
                    self.diag.add(
                        decl.name_pos.clone(),
                        format!("interface {:?} is already declared", decl.name),
                    );
                }
            }
            self.interfaces.insert(path.clone(), interfaces);

            let mut enums = BTreeMap::new();
            for decl in &package.enums {
                if enums
                    .insert(
                        decl.name.clone(),
                        DeclVisibility {
                            exported: decl.exported,
                        },
                    )
                    .is_some()
                {
                    self.diag.add(
                        decl.name_pos.clone(),
                        format!("enum {:?} is already declared", decl.name),
                    );
                }
            }
            self.enums.insert(path.clone(), enums);

            let mut functions = BTreeMap::new();
            for decl in &package.functions {
                if decl.receiver.is_some() {
                    continue;
                }
                if is_builtin_function(&decl.name) {
                    self.diag.add(
                        decl.name_pos.clone(),
                        format!("function {:?} is already declared", decl.name),
                    );
                    continue;
                }
                if functions
                    .insert(
                        decl.name.clone(),
                        DeclVisibility {
                            exported: decl.exported,
                        },
                    )
                    .is_some()
                {
                    self.diag.add(
                        decl.name_pos.clone(),
                        format!("function {:?} is already declared", decl.name),
                    );
                }
            }
            self.functions.insert(path.clone(), functions);

            let mut imports = BTreeMap::new();
            for import in &package.imports {
                if let Some(existing) = imports.insert(import.name.clone(), import.target.clone())
                    && existing != import.target
                {
                    self.diag.add(
                        import.decl.path_pos.clone(),
                        format!("import qualifier {:?} is ambiguous", import.name),
                    );
                }
            }
            self.imports.insert(path.clone(), imports);
        }
    }

    fn validate_exported_api(&mut self) {
        for (path, package) in &self.graph.packages {
            for decl in &package.structs {
                if !decl.exported {
                    continue;
                }
                for field in &decl.fields {
                    self.validate_exported_local_type_ref(
                        path,
                        &field.type_ref,
                        None,
                        "struct",
                        &decl.name,
                    );
                }
            }
            for decl in &package.interfaces {
                if !decl.exported {
                    continue;
                }
                for method in &decl.methods {
                    for param in &method.params {
                        self.validate_exported_local_type_ref(
                            path,
                            &param.type_ref,
                            None,
                            "interface",
                            &decl.name,
                        );
                    }
                    self.validate_exported_local_type_ref(
                        path,
                        &method.return_type,
                        None,
                        "interface",
                        &decl.name,
                    );
                }
            }
            for decl in &package.enums {
                if !decl.exported {
                    continue;
                }
                for case in &decl.cases {
                    for field in &case.fields {
                        self.validate_exported_local_type_ref(
                            path,
                            &field.type_ref,
                            None,
                            "enum",
                            &decl.name,
                        );
                    }
                }
            }
            for decl in &package.functions {
                if !decl.exported {
                    continue;
                }
                let type_params = type_param_set(&decl.type_params);
                let allowed = Some(&type_params);
                if let Some(receiver) = &decl.receiver {
                    self.validate_exported_local_type_ref(
                        path,
                        &receiver.type_ref,
                        allowed,
                        "method",
                        &decl.name,
                    );
                }
                for param in &decl.params {
                    self.validate_exported_local_type_ref(
                        path,
                        &param.type_ref,
                        allowed,
                        "function",
                        &decl.name,
                    );
                }
                self.validate_exported_local_type_ref(
                    path,
                    &decl.return_type,
                    allowed,
                    "function",
                    &decl.name,
                );
            }
        }
    }

    fn validate_exported_local_type_ref(
        &mut self,
        package_id: &PackageId,
        ref_: &TypeRef,
        type_params: Option<&BTreeSet<String>>,
        owner_kind: &str,
        owner_name: &str,
    ) {
        for arg in &ref_.type_args {
            self.validate_exported_local_type_ref(
                package_id,
                arg,
                type_params,
                owner_kind,
                owner_name,
            );
        }

        match ref_.kind {
            TypeRefKind::Errorable
            | TypeRefKind::Pointer
            | TypeRefKind::Array
            | TypeRefKind::Slice
            | TypeRefKind::Chan => {
                if let Some(elem) = ref_.elem.as_deref() {
                    self.validate_exported_local_type_ref(
                        package_id,
                        elem,
                        type_params,
                        owner_kind,
                        owner_name,
                    );
                }
                return;
            }
            TypeRefKind::Map => {
                if let Some(key) = ref_.key.as_deref() {
                    self.validate_exported_local_type_ref(
                        package_id,
                        key,
                        type_params,
                        owner_kind,
                        owner_name,
                    );
                }
                if let Some(value) = ref_.value.as_deref() {
                    self.validate_exported_local_type_ref(
                        package_id,
                        value,
                        type_params,
                        owner_kind,
                        owner_name,
                    );
                }
                return;
            }
            TypeRefKind::Function => {
                for param in &ref_.params {
                    self.validate_exported_local_type_ref(
                        package_id,
                        param,
                        type_params,
                        owner_kind,
                        owner_name,
                    );
                }
                if let Some(return_type) = ref_.return_type.as_deref() {
                    self.validate_exported_local_type_ref(
                        package_id,
                        return_type,
                        type_params,
                        owner_kind,
                        owner_name,
                    );
                }
                return;
            }
            TypeRefKind::Named => {}
        }

        if is_builtin_type(&ref_.name)
            || ref_.name.contains('.')
            || type_params.is_some_and(|params| params.contains(&ref_.name))
        {
            return;
        }

        let hidden_local_type = self
            .structs
            .get(package_id)
            .and_then(|decls| decls.get(&ref_.name))
            .or_else(|| {
                self.interfaces
                    .get(package_id)
                    .and_then(|decls| decls.get(&ref_.name))
            })
            .or_else(|| {
                self.enums
                    .get(package_id)
                    .and_then(|decls| decls.get(&ref_.name))
            })
            .is_some_and(|decl| !decl.exported);
        if hidden_local_type {
            self.diag.add(
                ref_.pos.clone(),
                format!(
                    "exported {owner_kind} {owner_name:?} cannot use non-exported type {:?}",
                    ref_.name
                ),
            );
        }
    }

    fn lower_interfaces(&mut self, package: &Package) -> Vec<InterfaceDecl> {
        package
            .interfaces
            .iter()
            .map(|decl| InterfaceDecl {
                interface_pos: decl.interface_pos.clone(),
                exported: decl.exported,
                name: canonical_decl_name(&self.graph.entry, package, &decl.name),
                name_pos: decl.name_pos.clone(),
                methods: decl
                    .methods
                    .iter()
                    .map(|method| InterfaceMethodDecl {
                        name: method.name.clone(),
                        name_pos: method.name_pos.clone(),
                        params: self.lower_params(package, &method.params, None),
                        return_type: self.rewrite_type_ref(package, &method.return_type, None),
                        return_is_bang: method.return_is_bang,
                    })
                    .collect(),
            })
            .collect()
    }

    fn lower_structs(&mut self, package: &Package) -> Vec<StructDecl> {
        package
            .structs
            .iter()
            .map(|decl| {
                let type_params = type_param_set(&decl.type_params);
                StructDecl {
                    struct_pos: decl.struct_pos.clone(),
                    exported: decl.exported,
                    resource: decl.resource,
                    name: canonical_decl_name(&self.graph.entry, package, &decl.name),
                    name_pos: decl.name_pos.clone(),
                    type_params: decl.type_params.clone(),
                    fields: decl
                        .fields
                        .iter()
                        .map(|field| StructField {
                            name: field.name.clone(),
                            name_pos: field.name_pos.clone(),
                            type_ref: self.rewrite_type_ref(
                                package,
                                &field.type_ref,
                                Some(&type_params),
                            ),
                        })
                        .collect(),
                }
            })
            .collect()
    }

    fn lower_enums(&mut self, package: &Package) -> Vec<EnumDecl> {
        package
            .enums
            .iter()
            .map(|decl| EnumDecl {
                enum_pos: decl.enum_pos.clone(),
                exported: decl.exported,
                name: canonical_decl_name(&self.graph.entry, package, &decl.name),
                name_pos: decl.name_pos.clone(),
                cases: decl
                    .cases
                    .iter()
                    .map(|case| EnumCaseDecl {
                        name: case.name.clone(),
                        name_pos: case.name_pos.clone(),
                        fields: case
                            .fields
                            .iter()
                            .map(|field| StructField {
                                name: field.name.clone(),
                                name_pos: field.name_pos.clone(),
                                type_ref: self.rewrite_type_ref(package, &field.type_ref, None),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect()
    }

    fn lower_functions(&mut self, package: &Package) -> Vec<FunctionDecl> {
        package
            .functions
            .iter()
            .map(|decl| {
                let type_params = type_param_set(&decl.type_params);
                let receiver = decl.receiver.as_ref().map(|receiver| ReceiverDecl {
                    name: receiver.name.clone(),
                    name_pos: receiver.name_pos.clone(),
                    type_ref: self.rewrite_type_ref(
                        package,
                        &receiver.type_ref,
                        Some(&type_params),
                    ),
                });
                let name = if receiver.is_some() {
                    decl.name.clone()
                } else {
                    canonical_function_name(self.graph, package, &decl.name)
                };
                FunctionDecl {
                    exported: decl.exported,
                    host_intrinsic: decl.host_intrinsic,
                    name,
                    name_pos: decl.name_pos.clone(),
                    type_params: decl.type_params.clone(),
                    receiver,
                    params: self.lower_params(package, &decl.params, Some(&type_params)),
                    return_type: self.rewrite_type_ref(
                        package,
                        &decl.return_type,
                        Some(&type_params),
                    ),
                    return_is_bang: decl.return_is_bang,
                    body: self.rewrite_block(package, &decl.body, Some(&type_params)),
                }
            })
            .collect()
    }

    fn lower_params(
        &mut self,
        package: &Package,
        params: &[Param],
        type_params: Option<&BTreeSet<String>>,
    ) -> Vec<Param> {
        params
            .iter()
            .map(|param| Param {
                name: param.name.clone(),
                name_pos: param.name_pos.clone(),
                type_ref: self.rewrite_type_ref(package, &param.type_ref, type_params),
            })
            .collect()
    }

    fn rewrite_type_ref(
        &mut self,
        package: &Package,
        ref_: &TypeRef,
        type_params: Option<&BTreeSet<String>>,
    ) -> TypeRef {
        let mut out = ref_.clone();
        out.type_args = ref_
            .type_args
            .iter()
            .map(|arg| self.rewrite_type_ref(package, arg, type_params))
            .collect();

        match ref_.kind {
            TypeRefKind::Errorable
            | TypeRefKind::Pointer
            | TypeRefKind::Array
            | TypeRefKind::Slice
            | TypeRefKind::Chan => {
                out.elem = ref_
                    .elem
                    .as_deref()
                    .map(|elem| Box::new(self.rewrite_type_ref(package, elem, type_params)));
                return out;
            }
            TypeRefKind::Map => {
                out.key = ref_
                    .key
                    .as_deref()
                    .map(|key| Box::new(self.rewrite_type_ref(package, key, type_params)));
                out.value = ref_
                    .value
                    .as_deref()
                    .map(|value| Box::new(self.rewrite_type_ref(package, value, type_params)));
                return out;
            }
            TypeRefKind::Function => {
                out.params = ref_
                    .params
                    .iter()
                    .map(|param| self.rewrite_type_ref(package, param, type_params))
                    .collect();
                out.return_type = ref_
                    .return_type
                    .as_deref()
                    .map(|ret| Box::new(self.rewrite_type_ref(package, ret, type_params)));
                return out;
            }
            TypeRefKind::Named => {}
        }

        if type_params.is_some_and(|params| params.contains(&ref_.name))
            || is_builtin_type(&ref_.name)
        {
            return out;
        }

        if let Some((import_name, member)) = ref_.name.split_once('.') {
            if let Some(target) = self.import_target(package, import_name)
                && let Some(type_kind) = self.type_kind(&target.id, member)
            {
                if !self.type_exported(&target.id, member) {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!(
                            "package {:?} does not export {type_kind} {member:?}",
                            target.name
                        ),
                    );
                    return out;
                }
                out.name = canonical_decl_name(&self.graph.entry, target, member);
            }
            return out;
        }

        if self.type_kind(&package.id, &ref_.name).is_some() {
            out.name = canonical_decl_name(&self.graph.entry, package, &ref_.name);
        }
        out
    }

    fn rewrite_block(
        &mut self,
        package: &Package,
        block: &BlockStmt,
        type_params: Option<&BTreeSet<String>>,
    ) -> BlockStmt {
        BlockStmt {
            lbrace: block.lbrace.clone(),
            stmts: block
                .stmts
                .iter()
                .map(|stmt| self.rewrite_statement(package, stmt, type_params))
                .collect(),
        }
    }

    fn rewrite_statement(
        &mut self,
        package: &Package,
        stmt: &Statement,
        type_params: Option<&BTreeSet<String>>,
    ) -> Statement {
        match stmt {
            Statement::Block(block) => {
                Statement::Block(Box::new(self.rewrite_block(package, block, type_params)))
            }
            Statement::Let(stmt) => Statement::Let(Box::new(LetStmt {
                let_pos: stmt.let_pos.clone(),
                name: stmt.name.clone(),
                name_pos: stmt.name_pos.clone(),
                value: self.rewrite_expr(package, &stmt.value, type_params),
            })),
            Statement::Var(stmt) => Statement::Var(Box::new(VarStmt {
                var_pos: stmt.var_pos.clone(),
                name: stmt.name.clone(),
                name_pos: stmt.name_pos.clone(),
                type_ref: self.rewrite_type_ref(package, &stmt.type_ref, type_params),
                value: stmt
                    .value
                    .as_ref()
                    .map(|expr| self.rewrite_expr(package, expr, type_params)),
            })),
            Statement::Assign(stmt) => Statement::Assign(Box::new(AssignStmt {
                target: self.rewrite_expr(package, &stmt.target, type_params),
                value: self.rewrite_expr(package, &stmt.value, type_params),
            })),
            Statement::CompoundAssign(stmt) => {
                Statement::CompoundAssign(Box::new(CompoundAssignStmt {
                    target: self.rewrite_expr(package, &stmt.target, type_params),
                    operator: stmt.operator,
                    op_pos: stmt.op_pos.clone(),
                    value: self.rewrite_expr(package, &stmt.value, type_params),
                }))
            }
            Statement::If(stmt) => Statement::If(Box::new(IfStmt {
                if_pos: stmt.if_pos.clone(),
                cond: self.rewrite_expr(package, &stmt.cond, type_params),
                then_block: self.rewrite_block(package, &stmt.then_block, type_params),
                else_stmt: stmt
                    .else_stmt
                    .as_ref()
                    .map(|stmt| self.rewrite_statement(package, stmt, type_params)),
            })),
            Statement::For(stmt) => Statement::For(Box::new(ForStmt {
                for_pos: stmt.for_pos.clone(),
                init: stmt
                    .init
                    .as_ref()
                    .map(|stmt| self.rewrite_statement(package, stmt, type_params)),
                cond: stmt
                    .cond
                    .as_ref()
                    .map(|expr| self.rewrite_expr(package, expr, type_params)),
                post: stmt
                    .post
                    .as_ref()
                    .map(|stmt| self.rewrite_statement(package, stmt, type_params)),
                body: self.rewrite_block(package, &stmt.body, type_params),
            })),
            Statement::Break(stmt) => Statement::Break(stmt.clone()),
            Statement::Continue(stmt) => Statement::Continue(stmt.clone()),
            Statement::Return(stmt) => Statement::Return(Box::new(ReturnStmt {
                return_pos: stmt.return_pos.clone(),
                value: stmt
                    .value
                    .as_ref()
                    .map(|expr| self.rewrite_expr(package, expr, type_params)),
            })),
            Statement::Match(stmt) => Statement::Match(Box::new(MatchStmt {
                match_pos: stmt.match_pos.clone(),
                value: self.rewrite_expr(package, &stmt.value, type_params),
                arms: stmt
                    .arms
                    .iter()
                    .map(|arm| MatchArm {
                        case_pos: arm.case_pos.clone(),
                        enum_type: self.rewrite_type_ref(package, &arm.enum_type, type_params),
                        case_name: arm.case_name.clone(),
                        case_name_pos: arm.case_name_pos.clone(),
                        bind_name: arm.bind_name.clone(),
                        bind_name_pos: arm.bind_name_pos.clone(),
                        bind_ignore: arm.bind_ignore,
                        body: self.rewrite_block(package, &arm.body, type_params),
                    })
                    .collect(),
                else_body: stmt
                    .else_body
                    .as_ref()
                    .map(|block| self.rewrite_block(package, block, type_params)),
            })),
            Statement::Expr(stmt) => Statement::Expr(Box::new(ExprStmt {
                expr: self.rewrite_expr(package, &stmt.expr, type_params),
            })),
            Statement::Spawn(stmt) => Statement::Spawn(Box::new(SpawnStmt {
                spawn_pos: stmt.spawn_pos.clone(),
                call: self.rewrite_expr(package, &stmt.call, type_params),
            })),
        }
    }

    fn rewrite_expr(
        &mut self,
        package: &Package,
        expr: &Expression,
        type_params: Option<&BTreeSet<String>>,
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
                inner: self.rewrite_expr(package, &expr.inner, type_params),
            })),
            Expression::FunctionLiteral(expr) => {
                Expression::FunctionLiteral(Box::new(FunctionLiteralExpr {
                    fn_pos: expr.fn_pos.clone(),
                    enclosing_function: expr.enclosing_function.clone(),
                    params: self.lower_params(package, &expr.params, type_params),
                    return_type: self.rewrite_type_ref(package, &expr.return_type, type_params),
                    return_is_bang: expr.return_is_bang,
                    body: self.rewrite_block(package, &expr.body, type_params),
                }))
            }
            Expression::Taskgroup(expr) => Expression::Taskgroup(Box::new(TaskgroupExpr {
                taskgroup_pos: expr.taskgroup_pos.clone(),
                result_type: self.rewrite_type_ref(package, &expr.result_type, type_params),
                body: self.rewrite_block(package, &expr.body, type_params),
            })),
            Expression::TypeApplication(expr) => {
                Expression::TypeApplication(Box::new(TypeApplicationExpr {
                    inner: self.rewrite_expr(package, &expr.inner, type_params),
                    lbracket_pos: expr.lbracket_pos.clone(),
                    type_args: expr
                        .type_args
                        .iter()
                        .map(|arg| self.rewrite_type_ref(package, arg, type_params))
                        .collect(),
                }))
            }
            Expression::Call(expr) => Expression::Call(Box::new(CallExpr {
                callee: self.rewrite_callee(package, &expr.callee, type_params),
                args: expr
                    .args
                    .iter()
                    .map(|arg| self.rewrite_expr(package, arg, type_params))
                    .collect(),
            })),
            Expression::Unary(expr) => Expression::Unary(Box::new(UnaryExpr {
                operator: expr.operator,
                op_pos: expr.op_pos.clone(),
                inner: self.rewrite_expr(package, &expr.inner, type_params),
            })),
            Expression::Binary(expr) => Expression::Binary(Box::new(BinaryExpr {
                left: self.rewrite_expr(package, &expr.left, type_params),
                operator: expr.operator,
                op_pos: expr.op_pos.clone(),
                right: self.rewrite_expr(package, &expr.right, type_params),
            })),
            Expression::Selector(expr) => {
                if let Some(rewritten) = self.rewrite_enum_case_selector(package, expr) {
                    return rewritten;
                }
                Expression::Selector(Box::new(SelectorExpr {
                    inner: self.rewrite_expr(package, &expr.inner, type_params),
                    dot_pos: expr.dot_pos.clone(),
                    name: expr.name.clone(),
                    name_pos: expr.name_pos.clone(),
                }))
            }
            Expression::Index(expr) => Expression::Index(Box::new(IndexExpr {
                inner: self.rewrite_expr(package, &expr.inner, type_params),
                lbracket_pos: expr.lbracket_pos.clone(),
                index: self.rewrite_expr(package, &expr.index, type_params),
            })),
            Expression::Slice(expr) => Expression::Slice(Box::new(SliceExpr {
                inner: self.rewrite_expr(package, &expr.inner, type_params),
                lbracket_pos: expr.lbracket_pos.clone(),
                start: expr
                    .start
                    .as_ref()
                    .map(|start| self.rewrite_expr(package, start, type_params)),
                colon_pos: expr.colon_pos.clone(),
                end: expr
                    .end
                    .as_ref()
                    .map(|end| self.rewrite_expr(package, end, type_params)),
            })),
            Expression::StructLiteral(expr) => {
                let type_ref = self
                    .rewrite_enum_case_type_ref(package, &expr.type_ref)
                    .unwrap_or_else(|| self.rewrite_type_ref(package, &expr.type_ref, type_params));
                Expression::StructLiteral(Box::new(StructLiteralExpr {
                    type_ref,
                    lbrace: expr.lbrace.clone(),
                    fields: expr
                        .fields
                        .iter()
                        .map(|field| StructLiteralField {
                            name: field.name.clone(),
                            name_pos: field.name_pos.clone(),
                            value: self.rewrite_expr(package, &field.value, type_params),
                        })
                        .collect(),
                }))
            }
            Expression::ArrayLiteral(expr) => {
                Expression::ArrayLiteral(Box::new(ArrayLiteralExpr {
                    type_ref: self.rewrite_type_ref(package, &expr.type_ref, type_params),
                    lbrace: expr.lbrace.clone(),
                    elements: expr
                        .elements
                        .iter()
                        .map(|element| self.rewrite_expr(package, element, type_params))
                        .collect(),
                }))
            }
            Expression::SliceLiteral(expr) => {
                Expression::SliceLiteral(Box::new(SliceLiteralExpr {
                    type_ref: self.rewrite_type_ref(package, &expr.type_ref, type_params),
                    lbrace: expr.lbrace.clone(),
                    elements: expr
                        .elements
                        .iter()
                        .map(|element| self.rewrite_expr(package, element, type_params))
                        .collect(),
                }))
            }
            Expression::MapLiteral(expr) => Expression::MapLiteral(Box::new(MapLiteralExpr {
                type_ref: self.rewrite_type_ref(package, &expr.type_ref, type_params),
                lbrace: expr.lbrace.clone(),
                pairs: expr
                    .pairs
                    .iter()
                    .map(|pair| MapLiteralPair {
                        key: self.rewrite_expr(package, &pair.key, type_params),
                        key_pos: pair.key_pos.clone(),
                        value: self.rewrite_expr(package, &pair.value, type_params),
                        value_pos: pair.value_pos.clone(),
                    })
                    .collect(),
            })),
            Expression::Propagate(expr) => Expression::Propagate(Box::new(PropagateExpr {
                inner: self.rewrite_expr(package, &expr.inner, type_params),
                question_pos: expr.question_pos.clone(),
            })),
            Expression::Handle(expr) => Expression::Handle(Box::new(HandleExpr {
                inner: self.rewrite_expr(package, &expr.inner, type_params),
                or_pos: expr.or_pos.clone(),
                err_name: expr.err_name.clone(),
                err_pos: expr.err_pos.clone(),
                handler: self.rewrite_block(package, &expr.handler, type_params),
            })),
            Expression::Missing(pos) => Expression::Missing(pos.clone()),
        }
    }

    fn rewrite_callee(
        &mut self,
        package: &Package,
        callee: &Expression,
        type_params: Option<&BTreeSet<String>>,
    ) -> Expression {
        if let Expression::TypeApplication(applied) = callee {
            return Expression::TypeApplication(Box::new(TypeApplicationExpr {
                inner: self.rewrite_callee(package, &applied.inner, type_params),
                lbracket_pos: applied.lbracket_pos.clone(),
                type_args: applied
                    .type_args
                    .iter()
                    .map(|arg| self.rewrite_type_ref(package, arg, type_params))
                    .collect(),
            }));
        }

        if let Expression::Ident(ident) = callee {
            if ident.name == "main" && package.id == self.graph.entry {
                return Expression::Ident(ident.clone());
            }
            if self
                .functions
                .get(&package.id)
                .is_some_and(|functions| functions.contains_key(&ident.name))
            {
                return Expression::Ident(Box::new(IdentExpr {
                    name: canonical_decl_name(&self.graph.entry, package, &ident.name),
                    name_pos: ident.name_pos.clone(),
                }));
            }
            return Expression::Ident(ident.clone());
        }

        if let Expression::Selector(selector) = callee
            && let Expression::Ident(inner) = &selector.inner
            && let Some(target) = self.import_target(package, &inner.name)
            && let Some(function) = self
                .functions
                .get(&target.id)
                .and_then(|functions| functions.get(&selector.name))
        {
            if !function.exported {
                self.diag.add(
                    selector.name_pos.clone(),
                    format!(
                        "package {:?} does not export function {:?}",
                        target.name, selector.name
                    ),
                );
                return Expression::Ident(Box::new(IdentExpr {
                    name: selector.name.clone(),
                    name_pos: selector.name_pos.clone(),
                }));
            }
            return Expression::Ident(Box::new(IdentExpr {
                name: canonical_decl_name(&self.graph.entry, target, &selector.name),
                name_pos: selector.name_pos.clone(),
            }));
        }

        self.rewrite_expr(package, callee, type_params)
    }

    fn rewrite_enum_case_selector(
        &mut self,
        package: &Package,
        expr: &SelectorExpr,
    ) -> Option<Expression> {
        let (parts, positions) = selector_path(&Expression::Selector(Box::new(expr.clone())))?;
        if parts.len() != 2 && parts.len() != 3 {
            return None;
        }

        let (target, enum_name, case_name) = if parts.len() == 2 {
            (package, parts[0].as_str(), parts[1].as_str())
        } else {
            let target = self.import_target(package, &parts[0])?;
            (target, parts[1].as_str(), parts[2].as_str())
        };

        if !self
            .enums
            .get(&target.id)
            .is_some_and(|enums| enums.contains_key(enum_name))
        {
            return None;
        }
        let target_id = target.id.clone();
        let target_name = target.name.clone();
        let canonical_enum_name = canonical_decl_name(&self.graph.entry, target, enum_name);
        if target_id != package.id && !self.type_exported(&target_id, enum_name) {
            self.diag.add(
                positions[positions.len() - 2].clone(),
                format!(
                    "package {:?} does not export enum {enum_name:?}",
                    target_name
                ),
            );
        }

        Some(Expression::Selector(Box::new(SelectorExpr {
            inner: Expression::Ident(Box::new(IdentExpr {
                name: canonical_enum_name,
                name_pos: positions[positions.len() - 2].clone(),
            })),
            dot_pos: expr.dot_pos.clone(),
            name: case_name.to_owned(),
            name_pos: positions[positions.len() - 1].clone(),
        })))
    }

    fn rewrite_enum_case_type_ref(&mut self, package: &Package, ref_: &TypeRef) -> Option<TypeRef> {
        let parts = ref_.name.split('.').collect::<Vec<_>>();
        if parts.len() != 2 && parts.len() != 3 {
            return None;
        }

        let (target, enum_name, case_name) = if parts.len() == 2 {
            (package, parts[0], parts[1])
        } else {
            let target = self.import_target(package, parts[0])?;
            (target, parts[1], parts[2])
        };
        if !self
            .enums
            .get(&target.id)
            .is_some_and(|enums| enums.contains_key(enum_name))
        {
            return None;
        }
        let target_id = target.id.clone();
        let target_name = target.name.clone();
        let canonical_enum_name = canonical_decl_name(&self.graph.entry, target, enum_name);
        if target_id != package.id && !self.type_exported(&target_id, enum_name) {
            self.diag.add(
                ref_.pos.clone(),
                format!(
                    "package {:?} does not export enum {enum_name:?}",
                    target_name
                ),
            );
        }

        Some(TypeRef::named(
            format!("{canonical_enum_name}.{case_name}"),
            ref_.pos.clone(),
        ))
    }

    fn type_kind(&self, package_id: &PackageId, name: &str) -> Option<&'static str> {
        if self
            .structs
            .get(package_id)
            .is_some_and(|decls| decls.contains_key(name))
        {
            return Some("type");
        }
        if self
            .interfaces
            .get(package_id)
            .is_some_and(|decls| decls.contains_key(name))
        {
            return Some("interface");
        }
        if self
            .enums
            .get(package_id)
            .is_some_and(|decls| decls.contains_key(name))
        {
            return Some("enum");
        }
        None
    }

    fn type_exported(&self, package_id: &PackageId, name: &str) -> bool {
        self.structs
            .get(package_id)
            .and_then(|decls| decls.get(name))
            .or_else(|| {
                self.interfaces
                    .get(package_id)
                    .and_then(|decls| decls.get(name))
            })
            .or_else(|| self.enums.get(package_id).and_then(|decls| decls.get(name)))
            .is_some_and(|decl| decl.exported)
    }

    fn import_target(&self, package: &Package, import_name: &str) -> Option<&Package> {
        let id = self.imports.get(&package.id)?.get(import_name)?;
        self.graph.packages.get(id)
    }
}

fn selector_path(expr: &Expression) -> Option<(Vec<String>, Vec<Position>)> {
    match expr {
        Expression::Ident(expr) => Some((vec![expr.name.clone()], vec![expr.name_pos.clone()])),
        Expression::Selector(expr) => {
            let (mut parts, mut positions) = selector_path(&expr.inner)?;
            parts.push(expr.name.clone());
            positions.push(expr.name_pos.clone());
            Some((parts, positions))
        }
        _ => None,
    }
}

fn type_param_set(params: &[TypeParam]) -> BTreeSet<String> {
    params.iter().map(|param| param.name.clone()).collect()
}

fn canonical_function_name(graph: &PackageGraph, package: &Package, name: &str) -> String {
    if package.id == graph.entry && name == "main" {
        return "main".to_owned();
    }
    canonical_decl_name(&graph.entry, package, name)
}

fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "void" | "noreturn" | "bool" | "i32" | "i64" | "str" | "error"
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::package::load_package_graph;

    use super::*;

    #[test]
    fn lowers_local_import_calls_to_canonical_names() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/imports_ok/main.yar"), false).unwrap();
        assert_eq!(diagnostics, Vec::new());

        let (program, diagnostics) = lower_package_graph(&graph);
        assert_eq!(diagnostics, Vec::new());

        assert!(
            program
                .functions
                .iter()
                .any(|function| function.name.ends_with(".lexer.classify"))
        );
        assert!(
            program
                .structs
                .iter()
                .any(|struct_| struct_.name.ends_with(".token.Kind"))
        );
        assert!(
            program
                .functions
                .iter()
                .any(|function| function.name == "main")
        );
    }

    #[test]
    fn lowers_stdlib_import_graph_to_one_program() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/stdlib_http/main.yar"), false).unwrap();
        assert_eq!(diagnostics, Vec::new());

        let (program, diagnostics) = lower_package_graph(&graph);
        assert_eq!(diagnostics, Vec::new());

        assert!(
            program
                .functions
                .iter()
                .any(|function| function.name == "http.serve")
        );
        assert!(
            program
                .functions
                .iter()
                .any(|function| function.name == "net.listen")
        );
        assert_eq!(
            program
                .structs
                .iter()
                .filter(|decl| decl.resource)
                .map(|decl| decl.name.as_str())
                .collect::<Vec<_>>(),
            vec!["net.Conn", "net.Listener"],
        );
        assert!(
            program
                .functions
                .iter()
                .find(|decl| decl.name == "net.listen")
                .is_some_and(|decl| decl.host_intrinsic)
        );
        assert!(
            program
                .functions
                .iter()
                .find(|decl| decl.name == "net.listen_stream")
                .is_some_and(|decl| !decl.host_intrinsic)
        );
    }

    #[test]
    fn lowers_every_testdata_entry_graph_without_diagnostics() {
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
            let (program, lower_diagnostics) = lower_package_graph(&graph);
            if !lower_diagnostics.is_empty() {
                failures.push(format!(
                    "{} lower: {:?}",
                    entry.display(),
                    lower_diagnostics
                ));
                continue;
            }
            if !program
                .functions
                .iter()
                .any(|function| function.name == "main")
            {
                failures.push(format!("{}: missing lowered main", entry.display()));
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
