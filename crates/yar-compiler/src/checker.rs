use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::*,
    diag::{Diagnostic, List},
    token::{Kind, Position},
};

pub type Type = String;

const TYPE_INVALID: &str = "";
const TYPE_VOID: &str = "void";
const TYPE_NORETURN: &str = "noreturn";
const TYPE_BOOL: &str = "bool";
const TYPE_I32: &str = "i32";
const TYPE_I64: &str = "i64";
const TYPE_STR: &str = "str";
const TYPE_ERROR: &str = "error";
const TYPE_NIL: &str = "nil";
const TYPE_UNTYPED_INT: &str = "untyped-int";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExprType {
    pub base: Type,
    pub errorable: bool,
}

impl ExprType {
    fn invalid() -> Self {
        Self {
            base: TYPE_INVALID.to_string(),
            errorable: false,
        }
    }

    fn plain(base: impl Into<Type>) -> Self {
        Self {
            base: base.into(),
            errorable: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub name: String,
    pub package: String,
    pub full_name: String,
    pub method: bool,
    pub receiver: Type,
    pub params: Vec<Type>,
    pub return_type: Type,
    pub errorable: bool,
    pub builtin: bool,
    pub host_intrinsic: bool,
    pub exported: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructField {
    pub exported: bool,
    pub name: String,
    pub type_: Type,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructInfo {
    pub name: String,
    pub exported: bool,
    pub resource: bool,
    pub fields: Vec<StructField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceMethodInfo {
    pub name: String,
    pub params: Vec<Type>,
    pub return_type: Type,
    pub errorable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceInfo {
    pub name: String,
    pub exported: bool,
    pub methods: Vec<InterfaceMethodInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumCaseInfo {
    pub name: String,
    pub tag: usize,
    pub payload_type: Type,
    pub fields: Vec<StructField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumInfo {
    pub name: String,
    pub exported: bool,
    pub cases: Vec<EnumCaseInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureInfo {
    pub name: String,
    pub type_: Type,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionLiteralInfo {
    pub signature: Signature,
    pub captures: Vec<CaptureInfo>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Info {
    pub functions: BTreeMap<String, Signature>,
    pub function_literals: BTreeMap<String, FunctionLiteralInfo>,
    pub methods: BTreeMap<Type, BTreeMap<String, Signature>>,
    pub structs: BTreeMap<String, StructInfo>,
    pub interfaces: BTreeMap<String, InterfaceInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    pub error_codes: BTreeMap<String, i32>,
    pub ordered_errors: Vec<String>,
}

pub fn check_program(program: &Program) -> (Info, Vec<Diagnostic>) {
    check_program_with_bodies(program, true)
}

pub fn check_program_metadata(program: &Program) -> (Info, Vec<Diagnostic>) {
    check_program_with_bodies(program, false)
}

fn check_program_with_bodies(program: &Program, check_bodies: bool) -> (Info, Vec<Diagnostic>) {
    let mut checker = Checker::new();
    checker.check_program(program, check_bodies);
    (checker.info, checker.diag.items())
}

struct Checker {
    diag: List,
    functions: BTreeMap<String, Signature>,
    function_signatures: BTreeMap<String, Signature>,
    struct_decls: BTreeMap<String, StructDecl>,
    interface_decls: BTreeMap<String, InterfaceDecl>,
    enum_decls: BTreeMap<String, EnumDecl>,
    current: Option<FunctionContext>,
    info: Info,
}

#[derive(Clone, Debug)]
struct FunctionContext {
    signature: Signature,
    literal_keys: Vec<String>,
    scopes: Vec<BTreeMap<String, LocalBinding>>,
    loop_depth: usize,
    closure_depth: usize,
    taskgroups: Vec<TaskgroupContext>,
}

#[derive(Clone, Debug)]
struct LocalBinding {
    type_: Type,
    closure_depth: usize,
}

#[derive(Clone, Debug)]
struct TaskgroupContext {
    result_type: Type,
    loop_depth: usize,
    closure_depth: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallUse {
    Ordinary,
    Spawn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TaskShareSafetyViolation {
    path: Vec<String>,
    type_: Type,
    resource: bool,
}

impl TaskShareSafetyViolation {
    fn disallowed(type_: impl Into<Type>) -> Self {
        Self {
            path: Vec::new(),
            type_: type_.into(),
            resource: false,
        }
    }

    fn resource(type_: impl Into<Type>) -> Self {
        Self {
            path: Vec::new(),
            type_: type_.into(),
            resource: true,
        }
    }

    fn prepend(mut self, segment: impl Into<String>) -> Self {
        self.path.insert(0, segment.into());
        self
    }

    fn describe(&self) -> String {
        let type_name = diagnostic_type_name(&self.type_);
        if self.path.is_empty() {
            if self.resource {
                return format!("resource type {type_name:?} is not share-safe");
            }
            return format!("type {type_name:?} is not share-safe");
        }

        let path = self.path.join(" -> ");
        if self.resource {
            format!("{path} contains non-share-safe resource type {type_name:?}")
        } else {
            format!("{path} has non-share-safe type {type_name:?}")
        }
    }
}

impl Checker {
    fn new() -> Self {
        Self {
            diag: List::default(),
            functions: builtin_functions(),
            function_signatures: BTreeMap::new(),
            struct_decls: BTreeMap::new(),
            interface_decls: BTreeMap::new(),
            enum_decls: BTreeMap::new(),
            current: None,
            info: Info::default(),
        }
    }

    fn check_program(&mut self, program: &Program, check_bodies: bool) {
        if program.package_name != "main" {
            self.diag
                .add(program.package_pos.clone(), "package must be main");
        }

        self.index_type_declarations(program);
        self.check_structs(program);
        self.check_interfaces(program);
        self.check_enums(program);
        self.check_type_cycles(program);
        self.check_functions(program);
        self.check_main(program);
        if check_bodies {
            self.check_function_bodies(program);
        }
        self.assign_error_codes();
    }

    fn index_type_declarations(&mut self, program: &Program) {
        for decl in &program.structs {
            if self.struct_decls.contains_key(&decl.name) {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!("struct {:?} is already declared", decl.name),
                );
                continue;
            }
            if self.interface_decls.contains_key(&decl.name)
                || self.enum_decls.contains_key(&decl.name)
            {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!("type {:?} is already declared", decl.name),
                );
                continue;
            }
            self.struct_decls.insert(decl.name.clone(), decl.clone());
        }
        for decl in &program.interfaces {
            if self.interface_decls.contains_key(&decl.name) {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!("interface {:?} is already declared", decl.name),
                );
                continue;
            }
            if self.struct_decls.contains_key(&decl.name)
                || self.enum_decls.contains_key(&decl.name)
            {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!("type {:?} is already declared", decl.name),
                );
                continue;
            }
            self.interface_decls.insert(decl.name.clone(), decl.clone());
        }
        for decl in &program.enums {
            if self.enum_decls.contains_key(&decl.name) {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!("enum {:?} is already declared", decl.name),
                );
                continue;
            }
            if self.interface_decls.contains_key(&decl.name)
                || self.struct_decls.contains_key(&decl.name)
            {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!("type {:?} is already declared", decl.name),
                );
                continue;
            }
            self.enum_decls.insert(decl.name.clone(), decl.clone());
        }
    }

    fn check_structs(&mut self, program: &Program) {
        for decl in &program.structs {
            if self.info.structs.contains_key(&decl.name) {
                continue;
            }
            if !decl.type_params.is_empty() {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!(
                        "generic struct {:?} must be monomorphized before checking",
                        decl.name
                    ),
                );
                continue;
            }
            let mut info = StructInfo {
                name: decl.name.clone(),
                exported: decl.exported,
                resource: decl.resource,
                fields: Vec::new(),
            };
            let mut seen_fields = BTreeSet::new();
            for field in &decl.fields {
                if !seen_fields.insert(field.name.clone()) {
                    self.diag.add(
                        field.name_pos.clone(),
                        format!(
                            "field {:?} is already declared in struct {:?}",
                            field.name, decl.name
                        ),
                    );
                    continue;
                }
                let field_type = self.resolve_type_ref(&field.type_ref);
                if matches!(
                    field_type.as_str(),
                    TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
                ) {
                    self.diag.add(
                        field.type_ref.pos.clone(),
                        format!(
                            "field {:?} cannot use type {:?}",
                            field.name,
                            field.type_ref.to_string()
                        ),
                    );
                    continue;
                }
                info.fields.push(StructField {
                    exported: field.exported,
                    name: field.name.clone(),
                    type_: field_type,
                });
            }
            self.info.structs.insert(decl.name.clone(), info);
        }
    }

    fn check_interfaces(&mut self, program: &Program) {
        for decl in &program.interfaces {
            if self.info.interfaces.contains_key(&decl.name) {
                continue;
            }
            let mut info = InterfaceInfo {
                name: decl.name.clone(),
                exported: decl.exported,
                methods: Vec::new(),
            };
            let mut seen_methods = BTreeSet::new();
            for method in &decl.methods {
                if !seen_methods.insert(method.name.clone()) {
                    self.diag.add(
                        method.name_pos.clone(),
                        format!(
                            "method {:?} is already declared in interface {:?}",
                            method.name, decl.name
                        ),
                    );
                    continue;
                }
                let return_type = self.resolve_type_ref(&method.return_type);
                if return_type == TYPE_NORETURN && method.return_is_bang {
                    self.diag.add(
                        method.return_type.pos.clone(),
                        "noreturn methods cannot also be errorable",
                    );
                    continue;
                }
                if return_type == TYPE_ERROR && method.return_is_bang {
                    self.diag.add(
                        method.return_type.pos.clone(),
                        "error methods cannot also be errorable",
                    );
                    continue;
                }
                let mut method_info = InterfaceMethodInfo {
                    name: method.name.clone(),
                    params: Vec::new(),
                    return_type,
                    errorable: method.return_is_bang,
                };
                let mut valid = true;
                for param in &method.params {
                    let param_type = self.resolve_type_ref(&param.type_ref);
                    if matches!(
                        param_type.as_str(),
                        TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
                    ) {
                        self.diag.add(
                            param.type_ref.pos.clone(),
                            format!(
                                "parameter {:?} cannot use type {:?}",
                                param.name,
                                param.type_ref.to_string()
                            ),
                        );
                        valid = false;
                        break;
                    }
                    method_info.params.push(param_type);
                }
                if valid {
                    info.methods.push(method_info);
                }
            }
            self.info.interfaces.insert(decl.name.clone(), info);
        }
    }

    fn check_enums(&mut self, program: &Program) {
        for decl in &program.enums {
            if self.info.enums.contains_key(&decl.name) {
                continue;
            }
            let mut info = EnumInfo {
                name: decl.name.clone(),
                exported: decl.exported,
                cases: Vec::new(),
            };
            let mut seen_cases = BTreeSet::new();
            for (tag, enum_case) in decl.cases.iter().enumerate() {
                if !seen_cases.insert(enum_case.name.clone()) {
                    self.diag.add(
                        enum_case.name_pos.clone(),
                        format!(
                            "case {:?} is already declared in enum {:?}",
                            enum_case.name, decl.name
                        ),
                    );
                    continue;
                }
                let mut case_info = EnumCaseInfo {
                    name: enum_case.name.clone(),
                    tag,
                    payload_type: String::new(),
                    fields: Vec::new(),
                };
                if !enum_case.fields.is_empty() {
                    let payload_name = enum_payload_type_name(&decl.name, &enum_case.name);
                    let mut payload = StructInfo {
                        name: payload_name.clone(),
                        exported: false,
                        resource: false,
                        fields: Vec::new(),
                    };
                    let mut seen_fields = BTreeSet::new();
                    for field in &enum_case.fields {
                        if !seen_fields.insert(field.name.clone()) {
                            self.diag.add(
                                field.name_pos.clone(),
                                format!(
                                    "field {:?} is already declared in enum case {:?}",
                                    field.name, enum_case.name
                                ),
                            );
                            continue;
                        }
                        let field_type = self.resolve_type_ref(&field.type_ref);
                        if matches!(
                            field_type.as_str(),
                            TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
                        ) {
                            self.diag.add(
                                field.type_ref.pos.clone(),
                                format!(
                                    "field {:?} cannot use type {:?}",
                                    field.name,
                                    field.type_ref.to_string()
                                ),
                            );
                            continue;
                        }
                        payload.fields.push(StructField {
                            exported: true,
                            name: field.name.clone(),
                            type_: field_type,
                        });
                    }
                    case_info.payload_type = payload_name.clone();
                    case_info.fields = payload.fields.clone();
                    self.info.structs.insert(payload_name, payload);
                }
                info.cases.push(case_info);
            }
            self.info.enums.insert(decl.name.clone(), info);
        }
    }

    fn check_functions(&mut self, program: &Program) {
        for function in &program.functions {
            if !function.type_params.is_empty() {
                self.diag.add(
                    function.name_pos.clone(),
                    format!(
                        "generic function {:?} must be monomorphized before checking",
                        function.name
                    ),
                );
                continue;
            }
            if function.receiver.is_none() && is_builtin_function(&function.name) {
                self.diag.add(
                    function.name_pos.clone(),
                    format!("function {:?} is already declared", function.name),
                );
                continue;
            }

            let mut sig = Signature {
                name: function.name.clone(),
                package: package_for_function(&function.name),
                full_name: function.name.clone(),
                method: false,
                receiver: String::new(),
                params: Vec::new(),
                return_type: self.resolve_type_ref(&function.return_type),
                errorable: function.return_is_bang,
                builtin: false,
                host_intrinsic: function.host_intrinsic,
                exported: function.exported,
            };

            if sig.return_type == TYPE_NORETURN && sig.errorable {
                self.diag.add(
                    function.return_type.pos.clone(),
                    "noreturn functions cannot also be errorable",
                );
            }
            if sig.return_type == TYPE_ERROR && sig.errorable {
                self.diag.add(
                    function.return_type.pos.clone(),
                    "error functions cannot also be errorable",
                );
            }

            if let Some(receiver) = &function.receiver {
                let receiver_type = self.resolve_method_receiver_type(&receiver.type_ref);
                if receiver_type == TYPE_INVALID {
                    continue;
                }
                sig.method = true;
                sig.receiver = receiver_type.clone();
                sig.full_name = method_full_name(&receiver_type, &function.name);
                sig.package = package_for_method_receiver(&receiver_type);
                sig.params.push(receiver_type.clone());
            } else if self.functions.contains_key(&function.name) {
                self.diag.add(
                    function.name_pos.clone(),
                    format!("function {:?} is already declared", function.name),
                );
                continue;
            }

            let mut seen_params = BTreeSet::new();
            if let Some(receiver) = &function.receiver {
                seen_params.insert(receiver.name.clone());
            }
            let mut valid_params = true;
            for param in &function.params {
                let param_type = self.resolve_type_ref(&param.type_ref);
                if matches!(
                    param_type.as_str(),
                    TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
                ) {
                    self.diag.add(
                        param.type_ref.pos.clone(),
                        format!(
                            "parameter {:?} cannot use type {:?}",
                            param.name,
                            param.type_ref.to_string()
                        ),
                    );
                    valid_params = false;
                    continue;
                }
                if !seen_params.insert(param.name.clone()) {
                    self.diag.add(
                        param.name_pos.clone(),
                        format!("duplicate parameter {:?}", param.name),
                    );
                    valid_params = false;
                    continue;
                }
                sig.params.push(param_type);
            }
            if !valid_params {
                continue;
            }

            if sig.method {
                let receiver_base = method_receiver_base_type(&sig.receiver);
                if self.lookup_method_for_base(&receiver_base, &function.name) {
                    self.diag.add(
                        function.name_pos.clone(),
                        format!(
                            "method {:?} is already declared for {:?}",
                            function.name, receiver_base
                        ),
                    );
                    continue;
                }
                self.info
                    .methods
                    .entry(sig.receiver.clone())
                    .or_default()
                    .insert(function.name.clone(), sig.clone());
                self.function_signatures
                    .insert(function_signature_key(function), sig);
            } else {
                self.functions.insert(function.name.clone(), sig.clone());
                self.info.functions.insert(function.name.clone(), sig);
                self.function_signatures.insert(
                    function_signature_key(function),
                    self.functions[&function.name].clone(),
                );
            }
        }
    }

    fn check_main(&mut self, program: &Program) {
        match self.functions.get("main") {
            Some(sig) if !sig.builtin && sig.return_type == TYPE_I32 => {}
            Some(_) => self
                .diag
                .add(program.package_pos.clone(), "main must return i32 or !i32"),
            None => self
                .diag
                .add(program.package_pos.clone(), "missing main function"),
        }
    }

    fn check_function_bodies(&mut self, program: &Program) {
        for function in &program.functions {
            let Some(signature) = self
                .function_signatures
                .get(&function_signature_key(function))
                .cloned()
            else {
                continue;
            };
            self.check_function_body(function, signature);
        }
    }

    fn check_function_body(&mut self, function: &FunctionDecl, signature: Signature) {
        let mut ctx = FunctionContext {
            signature: signature.clone(),
            literal_keys: Vec::new(),
            scopes: vec![BTreeMap::new()],
            loop_depth: 0,
            closure_depth: 0,
            taskgroups: Vec::new(),
        };

        let mut param_index = 0;
        if let Some(receiver) = &function.receiver {
            ctx.scopes[0].insert(
                receiver.name.clone(),
                LocalBinding {
                    type_: signature.params[0].clone(),
                    closure_depth: 0,
                },
            );
            param_index = 1;
        }
        for (idx, param) in function.params.iter().enumerate() {
            if let Some(param_type) = signature.params.get(idx + param_index) {
                ctx.scopes[0].insert(
                    param.name.clone(),
                    LocalBinding {
                        type_: param_type.clone(),
                        closure_depth: 0,
                    },
                );
            }
        }

        self.current = Some(ctx);
        self.check_block(&function.body);
        if signature.return_type != TYPE_VOID && !self.block_definitely_returns(&function.body) {
            if signature.return_type == TYPE_NORETURN {
                self.diag.add(
                    function.name_pos.clone(),
                    format!("function {:?} must not fall through", function.name),
                );
            } else {
                self.diag.add(
                    function.name_pos.clone(),
                    format!(
                        "function {:?} must return a value on all paths",
                        function.name
                    ),
                );
            }
        }
        self.current = None;
    }

    fn check_block(&mut self, block: &BlockStmt) {
        self.push_scope();
        for stmt in &block.stmts {
            self.check_statement(stmt);
        }
        self.pop_scope();
    }

    fn check_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Block(block) => self.check_block(block),
            Statement::Let(stmt) => self.check_let(stmt),
            Statement::Var(stmt) => self.check_var(stmt),
            Statement::Assign(stmt) => self.check_assign(stmt),
            Statement::CompoundAssign(stmt) => self.check_compound_assign(stmt),
            Statement::If(stmt) => {
                self.check_condition(
                    &stmt.cond,
                    "if condition must be bool",
                    "if condition cannot be errorable",
                );
                self.check_block(&stmt.then_block);
                if let Some(else_stmt) = &stmt.else_stmt {
                    self.check_statement(else_stmt);
                }
            }
            Statement::For(stmt) => {
                self.push_scope();
                if let Some(init) = &stmt.init {
                    self.check_statement(init);
                }
                if let Some(cond) = &stmt.cond {
                    self.check_condition(
                        cond,
                        "for condition must be bool",
                        "for condition cannot be errorable",
                    );
                }
                if let Some(post) = &stmt.post {
                    self.check_statement(post);
                }
                self.enter_loop();
                self.check_block(&stmt.body);
                self.exit_loop();
                self.pop_scope();
            }
            Statement::Break(stmt) => self.check_loop_control(&stmt.break_pos, "break"),
            Statement::Continue(stmt) => self.check_loop_control(&stmt.continue_pos, "continue"),
            Statement::Return(stmt) => self.check_return(stmt),
            Statement::Match(stmt) => self.check_match(stmt),
            Statement::Expr(stmt) => {
                let expr_type = self.check_expression(&stmt.expr);
                if expr_type.errorable {
                    self.diag.add(
                        stmt.expr.pos(),
                        "errorable value cannot be used as a statement",
                    );
                }
                if expr_type.base == TYPE_NORETURN && self.current_taskgroup().is_some() {
                    self.diag.add(
                        stmt.expr.pos(),
                        "noreturn expression cannot be used inside a taskgroup body because it would skip the taskgroup join",
                    );
                }
            }
            Statement::Spawn(stmt) => self.check_spawn(stmt),
        }
    }

    fn check_loop_control(&mut self, pos: &crate::token::Position, keyword: &str) {
        let Some(ctx) = self.current.as_ref() else {
            self.diag.add(
                pos.clone(),
                format!("{keyword} can only be used inside a loop"),
            );
            return;
        };
        if ctx.loop_depth == 0 {
            self.diag.add(
                pos.clone(),
                format!("{keyword} can only be used inside a loop"),
            );
            return;
        }
        if let Some(group) = self.current_taskgroup()
            && ctx.loop_depth == group.loop_depth
        {
            self.diag.add(
                pos.clone(),
                format!("{keyword} cannot exit a taskgroup body"),
            );
        }
    }

    fn check_let(&mut self, stmt: &LetStmt) {
        let mut value = self.check_expression(&stmt.value);
        value = self.require_non_errorable_value(
            &stmt.value,
            value,
            "errorable value cannot be bound to a local",
        );
        if value.base == TYPE_INVALID {
            return;
        }
        if value.base == TYPE_NIL {
            self.diag.add(
                stmt.value.pos(),
                "cannot infer type from nil without a pointer context",
            );
            return;
        }
        if value.base == TYPE_UNTYPED_INT {
            let default_type = self.default_untyped_integer_type(&stmt.value);
            if default_type == TYPE_INVALID {
                self.diag.add(
                    stmt.value.pos(),
                    "integer literal does not fit a supported integer type",
                );
                return;
            }
            value = self.coerce_untyped_integer(&stmt.value, value, &default_type);
        }
        if self.scope_owns(&stmt.name) {
            self.diag.add(
                stmt.name_pos.clone(),
                format!("local {:?} is already declared in this scope", stmt.name),
            );
            return;
        }
        self.bind_local(&stmt.name, value.base);
    }

    fn check_var(&mut self, stmt: &VarStmt) {
        let declared_type = self.resolve_type_ref(&stmt.type_ref);
        if matches!(
            declared_type.as_str(),
            TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
        ) {
            self.diag.add(
                stmt.type_ref.pos.clone(),
                format!(
                    "local {:?} cannot use type {:?}",
                    stmt.name,
                    stmt.type_ref.to_string()
                ),
            );
            return;
        }
        if self.scope_owns(&stmt.name) {
            self.diag.add(
                stmt.name_pos.clone(),
                format!("local {:?} is already declared in this scope", stmt.name),
            );
            return;
        }
        if let Some(value_expr) = &stmt.value {
            let mut value = self.check_expression(value_expr);
            value = self.require_non_errorable_value(
                value_expr,
                value,
                "errorable value cannot be bound to a local",
            );
            if value.base == TYPE_INVALID {
                return;
            }
            value = self.coerce_value(value_expr, value, &declared_type);
            if value.base == TYPE_INVALID {
                return;
            }
            if value.base != declared_type {
                self.diag.add(
                    value_expr.pos(),
                    format!(
                        "cannot assign {} to {}",
                        diagnostic_type_name(&value.base),
                        diagnostic_type_name(&declared_type)
                    ),
                );
                return;
            }
        }
        self.bind_local(&stmt.name, declared_type);
    }

    fn check_assign(&mut self, stmt: &AssignStmt) {
        if let Some(map_value_type) = self.check_map_assignment_target(&stmt.target) {
            if map_value_type == TYPE_INVALID {
                return;
            }
            let mut value = self.check_expression(&stmt.value);
            value = self.require_non_errorable_value(
                &stmt.value,
                value,
                "errorable value cannot be assigned directly",
            );
            if value.base == TYPE_INVALID {
                return;
            }
            value = self.coerce_value(&stmt.value, value, &map_value_type);
            if value.base == TYPE_INVALID {
                return;
            }
            if value.base != map_value_type {
                self.diag.add(
                    stmt.value.pos(),
                    format!(
                        "cannot assign {} to {}",
                        diagnostic_type_name(&value.base),
                        diagnostic_type_name(&map_value_type)
                    ),
                );
            }
            return;
        }

        let target_type = self.check_assignment_target(&stmt.target);
        if target_type == TYPE_INVALID {
            return;
        }
        let mut value = self.check_expression(&stmt.value);
        value = self.require_non_errorable_value(
            &stmt.value,
            value,
            "errorable value cannot be assigned directly",
        );
        if value.base == TYPE_INVALID {
            return;
        }
        value = self.coerce_value(&stmt.value, value, &target_type);
        if value.base == TYPE_INVALID {
            return;
        }
        if value.base != target_type {
            self.diag.add(
                stmt.value.pos(),
                format!(
                    "cannot assign {} to {}",
                    diagnostic_type_name(&value.base),
                    diagnostic_type_name(&target_type)
                ),
            );
        }
    }

    fn check_compound_assign(&mut self, stmt: &CompoundAssignStmt) {
        if self.check_map_assignment_target(&stmt.target).is_some() {
            self.diag.add(
                stmt.op_pos.clone(),
                "map compound assignment is not supported; use an explicit lookup and assignment",
            );
            return;
        }

        let target_type = self.check_assignment_target(&stmt.target);
        if target_type == TYPE_INVALID {
            return;
        }

        let mut value = self.check_expression(&stmt.value);
        value = self.require_non_errorable_value(
            &stmt.value,
            value,
            "compound assignment cannot use an errorable value",
        );
        if value.base == TYPE_INVALID {
            return;
        }
        value = self.coerce_value(&stmt.value, value, &target_type);

        let valid = if stmt.operator == Kind::Plus && target_type == TYPE_STR {
            value.base == TYPE_STR
        } else {
            is_integer_type(&target_type) && value.base == target_type
        };
        if !valid {
            self.diag.add(
                stmt.op_pos.clone(),
                if stmt.operator == Kind::Plus {
                    "'+=' requires matching integer or str operands"
                } else {
                    "compound arithmetic operators require matching integer operands"
                },
            );
        }
    }

    fn check_return(&mut self, stmt: &ReturnStmt) {
        let Some(signature) = self.current.as_ref().map(|ctx| ctx.signature.clone()) else {
            return;
        };
        if self.current_taskgroup().is_some() {
            self.diag.add(
                stmt.return_pos.clone(),
                "return is not allowed inside a taskgroup body",
            );
        }
        if signature.return_type == TYPE_NORETURN {
            self.diag
                .add(stmt.return_pos.clone(), "noreturn functions cannot return");
            return;
        }
        let Some(value_expr) = &stmt.value else {
            if signature.return_type == TYPE_VOID {
                return;
            }
            self.diag
                .add(stmt.return_pos.clone(), "return value is required");
            return;
        };

        if let Expression::Error(error) = value_expr {
            self.record_error(&error.name);
            if !signature.errorable && signature.return_type != TYPE_ERROR {
                self.diag.add(
                    value_expr.pos(),
                    format!(
                        "cannot return error.{} from function returning {}",
                        error.name, signature.return_type
                    ),
                );
            }
            return;
        }

        let mut value = self.check_expression(value_expr);
        if value.base == TYPE_INVALID {
            return;
        }
        if value.errorable {
            if signature.errorable && value.base == signature.return_type {
                return;
            }
            self.diag.add(
                value_expr.pos(),
                "return cannot use an errorable value directly",
            );
            return;
        }
        if matches!(value.base.as_str(), TYPE_NORETURN | TYPE_VOID) {
            self.diag.add(value_expr.pos(), "return requires a value");
            return;
        }
        value = self.coerce_value(value_expr, value, &signature.return_type);
        if value.base != signature.return_type {
            self.diag.add(
                value_expr.pos(),
                format!(
                    "cannot return {} from function returning {}",
                    diagnostic_type_name(&value.base),
                    diagnostic_type_name(&signature.return_type)
                ),
            );
        }
    }

    fn check_match(&mut self, stmt: &MatchStmt) {
        let value = self.check_expression(&stmt.value);
        if value.errorable {
            self.diag
                .add(stmt.value.pos(), "match value cannot be errorable");
            return;
        }
        let Some(enum_info) = self.info.enums.get(&value.base).cloned() else {
            self.diag
                .add(stmt.value.pos(), "match requires an enum value");
            return;
        };

        let mut seen = BTreeSet::new();
        for arm in &stmt.arms {
            let arm_enum = self.resolve_type_ref(&arm.enum_type);
            if arm_enum != value.base {
                self.diag.add(
                    arm.enum_type.pos.clone(),
                    format!("match arm must use enum {:?}", enum_info.name),
                );
            }

            let Some(enum_case) = enum_info
                .cases
                .iter()
                .find(|case| case.name == arm.case_name)
            else {
                self.diag.add(
                    arm.case_name_pos.clone(),
                    format!("enum {:?} has no case {:?}", enum_info.name, arm.case_name),
                );
                self.check_block(&arm.body);
                continue;
            };
            if !seen.insert(enum_case.name.clone()) {
                self.diag.add(
                    arm.case_name_pos.clone(),
                    format!("duplicate match arm for {:?}", enum_case.name),
                );
            }

            match (
                enum_case.fields.is_empty(),
                arm.bind_name.is_empty(),
                arm.bind_ignore,
            ) {
                (true, false, _) | (true, true, true) => {
                    self.diag.add(
                        arm.bind_name_pos.clone(),
                        format!("plain enum case {:?} cannot bind a payload", enum_case.name),
                    );
                    self.check_block(&arm.body);
                }
                (false, false, false) => {
                    self.push_scope();
                    self.bind_local(&arm.bind_name, enum_case.payload_type.clone());
                    self.check_block_in_current_scope(&arm.body);
                    self.pop_scope();
                }
                _ => self.check_block(&arm.body),
            }
        }

        if let Some(else_body) = &stmt.else_body {
            self.check_block(else_body);
            return;
        }

        if seen.len() == enum_info.cases.len() {
            return;
        }
        let missing = enum_info
            .cases
            .iter()
            .filter(|case| !seen.contains(&case.name))
            .map(|case| format!("{}.{}", enum_info.name, case.name))
            .collect::<Vec<_>>()
            .join(", ");
        self.diag.add(
            stmt.match_pos.clone(),
            format!(
                "match on {:?} is not exhaustive; missing {missing}",
                enum_info.name
            ),
        );
    }

    fn check_condition(&mut self, expr: &Expression, type_message: &str, error_message: &str) {
        let cond = self.check_expression(expr);
        if cond.errorable {
            self.diag.add(expr.pos(), error_message);
        }
        if cond.base != TYPE_BOOL {
            self.diag.add(expr.pos(), type_message);
        }
    }

    fn check_expression(&mut self, expr: &Expression) -> ExprType {
        match expr {
            Expression::Ident(expr) => match self.lookup_local(&expr.name) {
                Some(binding) => {
                    if self.is_captured_local(&expr.name) {
                        self.record_capture(&expr.name, &binding.type_, binding.closure_depth);
                    }
                    ExprType::plain(binding.type_)
                }
                None => {
                    self.diag.add(
                        expr.name_pos.clone(),
                        format!("unknown local {:?}", expr.name),
                    );
                    ExprType::invalid()
                }
            },
            Expression::Int(_) => ExprType::plain(TYPE_UNTYPED_INT),
            Expression::Char(_) => ExprType::plain(TYPE_I32),
            Expression::String(_) => ExprType::plain(TYPE_STR),
            Expression::Bool(_) => ExprType::plain(TYPE_BOOL),
            Expression::Nil(_) => ExprType::plain(TYPE_NIL),
            Expression::Error(expr) => {
                self.record_error(&expr.name);
                ExprType::plain(TYPE_ERROR)
            }
            Expression::Group(expr) => self.check_expression(&expr.inner),
            Expression::Unary(expr) => self.check_unary(expr),
            Expression::Binary(expr) => self.check_binary(expr),
            Expression::Selector(expr) => self.check_selector(expr),
            Expression::Index(expr) => self.check_index(expr),
            Expression::Slice(expr) => self.check_slice_expr(expr),
            Expression::StructLiteral(expr) => self.check_struct_literal(expr),
            Expression::ArrayLiteral(expr) => self.check_array_literal(expr),
            Expression::SliceLiteral(expr) => self.check_slice_literal(expr),
            Expression::MapLiteral(expr) => self.check_map_literal(expr),
            Expression::Call(expr) => self.check_call(expr, CallUse::Ordinary),
            Expression::Propagate(expr) => self.check_propagate(expr),
            Expression::Handle(expr) => self.check_handle(expr),
            Expression::FunctionLiteral(expr) => self.check_function_literal(expr),
            Expression::Taskgroup(expr) => self.check_taskgroup(expr),
            Expression::TypeApplication(expr) => {
                self.diag.add(
                    expr.lbracket_pos.clone(),
                    "type arguments are not valid in expression position",
                );
                ExprType::invalid()
            }
            Expression::Missing(pos) => {
                self.diag.add(pos.clone(), "missing expression");
                ExprType::invalid()
            }
        }
    }

    fn check_propagate(&mut self, expr: &PropagateExpr) -> ExprType {
        let inner = self.check_expression(&expr.inner);
        if self.current_taskgroup().is_some() {
            self.diag.add(
                expr.question_pos.clone(),
                "? is not allowed inside a taskgroup body because it could skip the taskgroup join",
            );
            return ExprType::invalid();
        }
        let value_type = if inner.errorable {
            Some(inner.base)
        } else if let Some(value_type) = parse_errorable_type(&inner.base) {
            Some(value_type)
        } else if inner.base == TYPE_ERROR {
            Some(TYPE_VOID.to_string())
        } else {
            None
        };

        if let Some(value_type) = value_type {
            if !self.current_can_propagate_error() {
                self.diag.add(
                    expr.question_pos.clone(),
                    "cannot use ? in a function that cannot return an error",
                );
                return ExprType::invalid();
            }
            return ExprType::plain(value_type);
        }

        self.diag.add(
            expr.question_pos.clone(),
            "? requires an errorable expression or error value",
        );
        ExprType::invalid()
    }

    fn check_handle(&mut self, expr: &HandleExpr) -> ExprType {
        let inner = self.check_expression(&expr.inner);
        let value_type = if inner.errorable {
            inner.base
        } else if let Some(value_type) = parse_errorable_type(&inner.base) {
            value_type
        } else if inner.base == TYPE_ERROR {
            TYPE_VOID.to_string()
        } else {
            self.diag.add(
                expr.or_pos.clone(),
                "or requires an errorable expression or error value",
            );
            return ExprType::invalid();
        };
        let requires_terminating_handler = value_type != TYPE_VOID;

        self.push_scope();
        self.bind_local(&expr.err_name, TYPE_ERROR.to_string());
        for stmt in &expr.handler.stmts {
            self.check_statement(stmt);
        }
        self.pop_scope();
        if requires_terminating_handler && !self.block_terminates_control_flow(&expr.handler) {
            self.diag.add(
                expr.or_pos.clone(),
                "or handler for a value result must terminate control flow",
            );
            return ExprType::invalid();
        }
        ExprType::plain(value_type)
    }

    fn check_unary(&mut self, unary: &UnaryExpr) -> ExprType {
        match unary.operator {
            Kind::Minus => {
                let inner = self.check_expression(&unary.inner);
                if inner.errorable {
                    self.diag.add(
                        unary.op_pos.clone(),
                        "unary operators cannot use errorable operands",
                    );
                    return ExprType::invalid();
                }
                if !is_integer_type(&inner.base) {
                    self.diag
                        .add(unary.op_pos.clone(), "unary - requires an integer operand");
                    return ExprType::invalid();
                }
                inner
            }
            Kind::Bang => {
                let inner = self.check_expression(&unary.inner);
                if inner.errorable {
                    self.diag.add(
                        unary.op_pos.clone(),
                        "unary operators cannot use errorable operands",
                    );
                    return ExprType::invalid();
                }
                if inner.base != TYPE_BOOL {
                    self.diag
                        .add(unary.op_pos.clone(), "unary ! requires a bool operand");
                    return ExprType::invalid();
                }
                ExprType::plain(TYPE_BOOL)
            }
            Kind::Amp => {
                if let Some(name) = self.address_root_captured_outer_local(&unary.inner) {
                    self.diag.add(
                        unary.op_pos.clone(),
                        format!("captured outer local {name:?} is not addressable"),
                    );
                    return ExprType::invalid();
                }
                let type_ = self.check_addressable_expr(&unary.inner, true);
                if type_ == TYPE_INVALID {
                    self.diag.add(
                        unary.op_pos.clone(),
                        "address-of requires an addressable operand or composite literal",
                    );
                    return ExprType::invalid();
                }
                ExprType::plain(format!("*{type_}"))
            }
            Kind::Star => {
                let inner = self.check_expression(&unary.inner);
                if inner.errorable {
                    self.diag.add(
                        unary.op_pos.clone(),
                        "dereference cannot use an errorable operand",
                    );
                    return ExprType::invalid();
                }
                match parse_pointer_type(&inner.base) {
                    Some(elem) => ExprType::plain(elem),
                    None => {
                        self.diag.add(
                            unary.op_pos.clone(),
                            "dereference requires a pointer operand",
                        );
                        ExprType::invalid()
                    }
                }
            }
            _ => {
                self.diag
                    .add(unary.op_pos.clone(), "unsupported unary operator");
                ExprType::invalid()
            }
        }
    }

    fn check_binary(&mut self, binary: &BinaryExpr) -> ExprType {
        let left = self.check_expression(&binary.left);
        let right = self.check_expression(&binary.right);
        if left.errorable || right.errorable {
            self.diag.add(
                binary.op_pos.clone(),
                "binary operators cannot use errorable operands",
            );
            return ExprType::invalid();
        }

        match binary.operator {
            Kind::AmpAmp | Kind::PipePipe => {
                if left.base != TYPE_BOOL || right.base != TYPE_BOOL {
                    self.diag.add(
                        binary.op_pos.clone(),
                        "logical operators require bool operands",
                    );
                    return ExprType::invalid();
                }
                ExprType::plain(TYPE_BOOL)
            }
            Kind::Plus if left.base == TYPE_STR && right.base == TYPE_STR => {
                ExprType::plain(TYPE_STR)
            }
            Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash | Kind::Percent => {
                match self.coerce_binary_integers(&binary.left, left, &binary.right, right) {
                    Some(result) => ExprType::plain(result),
                    None => {
                        self.diag.add(
                            binary.op_pos.clone(),
                            if binary.operator == Kind::Plus {
                                "'+' requires matching integer or str operands"
                            } else {
                                "arithmetic operators require matching integer operands"
                            },
                        );
                        ExprType::invalid()
                    }
                }
            }
            Kind::Less | Kind::LessEqual | Kind::Greater | Kind::GreaterEqual => {
                if self
                    .coerce_binary_integers(&binary.left, left, &binary.right, right)
                    .is_none()
                {
                    self.diag.add(
                        binary.op_pos.clone(),
                        "relational operators require matching integer operands",
                    );
                    return ExprType::invalid();
                }
                ExprType::plain(TYPE_BOOL)
            }
            Kind::EqualEqual | Kind::BangEqual => {
                if is_integer_type(&left.base) || is_integer_type(&right.base) {
                    if self
                        .coerce_binary_integers(&binary.left, left, &binary.right, right)
                        .is_none()
                    {
                        self.diag.add(
                            binary.op_pos.clone(),
                            "comparison operands must have the same type",
                        );
                        return ExprType::invalid();
                    }
                    return ExprType::plain(TYPE_BOOL);
                }
                if self.info.enums.contains_key(&left.base)
                    || self.info.enums.contains_key(&right.base)
                {
                    if left.base != right.base {
                        self.diag.add(
                            binary.op_pos.clone(),
                            "comparison operands must have the same type",
                        );
                    } else {
                        self.diag.add(
                            binary.op_pos.clone(),
                            "comparison is not supported for enum values in v0.4",
                        );
                    }
                    return ExprType::invalid();
                }
                if left.base == TYPE_NIL && right.base == TYPE_NIL {
                    self.diag.add(
                        binary.op_pos.clone(),
                        "comparison is only supported for bool, integers, pointers, str, and error",
                    );
                    return ExprType::invalid();
                }
                if matches!(
                    left.base.as_str(),
                    TYPE_BOOL | TYPE_STR | TYPE_ERROR | TYPE_NIL
                ) || is_pointer_type(&left.base)
                    || parse_chan_type(&left.base).is_some()
                {
                    if !self.same_comparable_type(&left.base, &right.base) {
                        self.diag.add(
                            binary.op_pos.clone(),
                            "comparison operands must have the same type",
                        );
                        return ExprType::invalid();
                    }
                    return ExprType::plain(TYPE_BOOL);
                }
                if left.base != right.base {
                    self.diag.add(
                        binary.op_pos.clone(),
                        "comparison operands must have the same type",
                    );
                } else {
                    self.diag.add(
                        binary.op_pos.clone(),
                        "comparison is only supported for bool, integers, pointers, str, and error",
                    );
                }
                ExprType::invalid()
            }
            _ => {
                self.diag.add(binary.op_pos.clone(), "unsupported operator");
                ExprType::invalid()
            }
        }
    }

    fn check_selector(&mut self, selector: &SelectorExpr) -> ExprType {
        if let Expression::Ident(ident) = &selector.inner
            && self.lookup_local(&ident.name).is_none()
            && let Some(enum_info) = self.info.enums.get(&ident.name)
        {
            if let Some(enum_case) = enum_info
                .cases
                .iter()
                .find(|case| case.name == selector.name)
            {
                if !enum_case.fields.is_empty() {
                    self.diag.add(
                        selector.name_pos.clone(),
                        format!(
                            "payload case {:?} requires a constructor body",
                            selector.name
                        ),
                    );
                    return ExprType::invalid();
                }
                return ExprType::plain(enum_info.name.clone());
            }
            self.diag.add(
                selector.name_pos.clone(),
                format!("enum {:?} has no case {:?}", enum_info.name, selector.name),
            );
            return ExprType::invalid();
        }

        let inner = self.check_expression(&selector.inner);
        if inner.errorable {
            self.diag.add(
                selector.dot_pos.clone(),
                "field access cannot use an errorable value",
            );
            return ExprType::invalid();
        }
        let base = parse_pointer_type(&inner.base).unwrap_or_else(|| inner.base.clone());
        if let Some(interface) = self.info.interfaces.get(&base) {
            if interface
                .methods
                .iter()
                .any(|method| method.name == selector.name)
            {
                self.diag
                    .add(selector.name_pos.clone(), "method values are not supported");
                return ExprType::invalid();
            }
            self.diag.add(
                selector.name_pos.clone(),
                format!(
                    "interface {:?} has no method {:?}",
                    interface.name, selector.name
                ),
            );
            return ExprType::invalid();
        }

        let Some(info) = self.info.structs.get(&base).cloned() else {
            self.diag.add(
                selector.dot_pos.clone(),
                "field access requires a struct value",
            );
            return ExprType::invalid();
        };
        match info
            .fields
            .iter()
            .find(|field| field.name == selector.name)
            .cloned()
        {
            Some(field) => {
                if self.private_field_is_inaccessible(&info, &field, &selector.name_pos) {
                    ExprType::invalid()
                } else {
                    ExprType::plain(field.type_)
                }
            }
            None => {
                if self.lookup_method_for_base(&base, &selector.name) {
                    self.diag
                        .add(selector.name_pos.clone(), "method values are not supported");
                    return ExprType::invalid();
                }
                self.diag.add(
                    selector.name_pos.clone(),
                    format!(
                        "struct {:?} has no field {:?}",
                        diagnostic_type_name(&info.name),
                        selector.name
                    ),
                );
                ExprType::invalid()
            }
        }
    }

    fn current_package(&self) -> &str {
        self.current
            .as_ref()
            .map(|current| current.signature.package.as_str())
            .unwrap_or_default()
    }

    fn private_field_is_inaccessible(
        &mut self,
        info: &StructInfo,
        field: &StructField,
        field_pos: &Position,
    ) -> bool {
        let package = package_for_type(&info.name);
        if field.exported || package.is_empty() || self.current_package() == package {
            return false;
        }
        self.diag.add(
            field_pos.clone(),
            format!(
                "field {:?} of struct {:?} is not exported by package {:?}",
                field.name,
                diagnostic_type_name(&info.name),
                package_display_name(&package),
            ),
        );
        true
    }

    fn check_index(&mut self, index: &IndexExpr) -> ExprType {
        let inner = self.check_expression(&index.inner);
        if inner.errorable {
            self.diag.add(
                index.lbracket_pos.clone(),
                "indexing cannot use an errorable value",
            );
            return ExprType::invalid();
        }

        if let Some((key_type, value_type)) = parse_map_type(&inner.base) {
            let key = self.check_expression(&index.index);
            if key.errorable {
                self.diag
                    .add(index.index.pos(), "map key expression cannot be errorable");
                return ExprType::invalid();
            }
            let key = self.coerce_value(&index.index, key, &key_type);
            if key.base != key_type {
                self.diag.add(
                    index.index.pos(),
                    format!(
                        "map key must be {}, got {}",
                        diagnostic_type_name(&key_type),
                        diagnostic_type_name(&key.base)
                    ),
                );
                return ExprType::invalid();
            }
            self.record_error("MissingKey");
            return ExprType {
                base: value_type,
                errorable: true,
            };
        }

        let elem_type = if inner.base == TYPE_STR {
            TYPE_I32.to_string()
        } else if let Some(elem) = sequence_element_type(&inner.base) {
            elem
        } else {
            self.diag.add(
                index.lbracket_pos.clone(),
                "indexing requires an array, slice, map, or str value",
            );
            return ExprType::invalid();
        };
        if !self.check_index_like_integer(&index.index) {
            return ExprType::invalid();
        }
        ExprType::plain(elem_type)
    }

    fn check_slice_expr(&mut self, slice: &SliceExpr) -> ExprType {
        let inner = self.check_expression(&slice.inner);
        if inner.errorable {
            self.diag.add(
                slice.lbracket_pos.clone(),
                "slicing cannot use an errorable value",
            );
            return ExprType::invalid();
        }
        let out_type = if inner.base == TYPE_STR {
            TYPE_STR.to_string()
        } else if parse_slice_type(&inner.base).is_some() {
            inner.base
        } else {
            self.diag.add(
                slice.lbracket_pos.clone(),
                "slicing requires a slice or str value",
            );
            return ExprType::invalid();
        };
        if let Some(start) = &slice.start {
            self.check_slice_bound(start);
        }
        if let Some(end) = &slice.end {
            self.check_slice_bound(end);
        }
        ExprType::plain(out_type)
    }

    fn check_struct_literal(&mut self, literal: &StructLiteralExpr) -> ExprType {
        let type_ = self.resolve_type_ref(&literal.type_ref);
        let Some(info) = self.info.structs.get(&type_).cloned() else {
            self.diag.add(
                literal.type_ref.pos.clone(),
                "struct literal requires a struct type",
            );
            return ExprType::invalid();
        };
        let package = package_for_type(&info.name);
        if info.fields.iter().any(|field| !field.exported)
            && !package.is_empty()
            && self.current_package() != package
        {
            self.diag.add(
                literal.type_ref.pos.clone(),
                format!(
                    "struct literal for {:?} is not allowed outside package {:?} because it has package-private fields",
                    diagnostic_type_name(&info.name),
                    package_display_name(&package),
                ),
            );
            return ExprType::invalid();
        }
        let mut seen = BTreeSet::new();
        for field in &literal.fields {
            if !seen.insert(field.name.clone()) {
                self.diag.add(
                    field.name_pos.clone(),
                    format!("field {:?} is already initialized", field.name),
                );
                continue;
            }
            let Some(field_info) = info
                .fields
                .iter()
                .find(|candidate| candidate.name == field.name)
            else {
                self.diag.add(
                    field.name_pos.clone(),
                    format!(
                        "struct {:?} has no field {:?}",
                        diagnostic_type_name(&info.name),
                        field.name
                    ),
                );
                continue;
            };
            let mut value = self.check_expression(&field.value);
            value = self.require_non_errorable_value(
                &field.value,
                value,
                "errorable value cannot be used in a struct literal",
            );
            value = self.coerce_value(&field.value, value, &field_info.type_);
            if value.base != field_info.type_ {
                self.diag.add(
                    field.value.pos(),
                    format!(
                        "cannot assign {} to {}",
                        diagnostic_type_name(&value.base),
                        diagnostic_type_name(&field_info.type_)
                    ),
                );
            }
        }
        ExprType::plain(self.enum_type_for_payload(&type_).unwrap_or(type_))
    }

    fn check_array_literal(&mut self, literal: &ArrayLiteralExpr) -> ExprType {
        let type_ = self.resolve_type_ref(&literal.type_ref);
        let Some((len, elem_type)) = parse_array_type(&type_) else {
            self.diag.add(
                literal.type_ref.pos.clone(),
                "array literal requires an array type",
            );
            return ExprType::invalid();
        };
        if literal.elements.len() > len {
            self.diag.add(
                literal.type_ref.pos.clone(),
                format!("array literal has too many elements for {type_}"),
            );
        }
        for element in &literal.elements {
            self.check_aggregate_element(element, &elem_type, "array literal");
        }
        ExprType::plain(type_)
    }

    fn check_slice_literal(&mut self, literal: &SliceLiteralExpr) -> ExprType {
        let type_ = self.resolve_type_ref(&literal.type_ref);
        let Some(elem_type) = parse_slice_type(&type_) else {
            self.diag.add(
                literal.type_ref.pos.clone(),
                "slice literal requires a slice type",
            );
            return ExprType::invalid();
        };
        for element in &literal.elements {
            self.check_aggregate_element(element, &elem_type, "slice literal");
        }
        ExprType::plain(type_)
    }

    fn check_map_literal(&mut self, literal: &MapLiteralExpr) -> ExprType {
        let type_ = self.resolve_type_ref(&literal.type_ref);
        let Some((key_type, value_type)) = parse_map_type(&type_) else {
            self.diag.add(
                literal.type_ref.pos.clone(),
                "map literal requires a map type",
            );
            return ExprType::invalid();
        };
        for pair in &literal.pairs {
            let mut key = self.check_expression(&pair.key);
            key = self.require_non_errorable_value(
                &pair.key,
                key,
                "errorable value cannot be used as a map key",
            );
            key = self.coerce_value(&pair.key, key, &key_type);
            if key.base != key_type {
                self.diag.add(
                    pair.key.pos(),
                    format!(
                        "cannot use {} as map key type {}",
                        diagnostic_type_name(&key.base),
                        diagnostic_type_name(&key_type)
                    ),
                );
            }
            let mut value = self.check_expression(&pair.value);
            value = self.require_non_errorable_value(
                &pair.value,
                value,
                "errorable value cannot be used as a map value",
            );
            value = self.coerce_value(&pair.value, value, &value_type);
            if value.base != value_type {
                self.diag.add(
                    pair.value.pos(),
                    format!(
                        "cannot assign {} to {}",
                        diagnostic_type_name(&value.base),
                        diagnostic_type_name(&value_type)
                    ),
                );
            }
        }
        ExprType::plain(type_)
    }

    fn check_call(&mut self, call: &CallExpr, call_use: CallUse) -> ExprType {
        if let Some(result) = self.check_chan_new_call(call) {
            if call_use == CallUse::Spawn {
                self.diag.add(
                    call.callee.pos(),
                    "spawn does not currently support builtin \"chan_new\" directly; wrap it in an inline function literal",
                );
            }
            return result;
        }
        if let Some((name, name_pos)) = call_name(&call.callee) {
            if let Some(binding) = self.lookup_local(&name) {
                if self.is_captured_local(&name) {
                    self.record_capture(&name, &binding.type_, binding.closure_depth);
                }
                let Some(function_type) = parse_function_type(&binding.type_) else {
                    self.diag.add(name_pos, "call target must be callable");
                    return ExprType::invalid();
                };
                let sig = Signature {
                    name: "function value".to_string(),
                    package: String::new(),
                    full_name: "function value".to_string(),
                    method: false,
                    receiver: String::new(),
                    params: function_type.params,
                    return_type: function_type.return_type,
                    errorable: function_type.errorable,
                    builtin: false,
                    host_intrinsic: false,
                    exported: false,
                };
                if call_use == CallUse::Spawn {
                    self.diag.add(
                        name_pos.clone(),
                        "spawn cannot call an arbitrary function value; use a named function or an inline function literal with share-safe captures",
                    );
                }
                self.check_call_args(
                    &sig.name.clone(),
                    &name_pos,
                    &sig,
                    call,
                    0,
                    CallUse::Ordinary,
                );
                return ExprType {
                    base: sig.return_type,
                    errorable: sig.errorable,
                };
            }
            if let Some(result) = self.check_special_builtin_call(&name, &name_pos, call) {
                if call_use == CallUse::Spawn {
                    self.diag.add(
                        name_pos,
                        format!(
                            "spawn does not currently support builtin {name:?} directly; wrap it in an inline function literal"
                        ),
                    );
                }
                return result;
            }

            let Some(sig) = self.functions.get(&name).cloned() else {
                self.diag
                    .add(name_pos, format!("unknown function {:?}", name));
                return ExprType::invalid();
            };
            let arg_use = if call_use == CallUse::Spawn {
                if sig.builtin {
                    self.diag.add(
                        name_pos.clone(),
                        format!(
                            "spawn does not currently support builtin {name:?} directly; wrap it in an inline function literal"
                        ),
                    );
                    CallUse::Ordinary
                } else if sig.host_intrinsic && sig.full_name != "fs.read_file" {
                    self.diag.add(
                        name_pos.clone(),
                        format!(
                            "spawn does not currently support host intrinsic {:?} directly; wrap it in an inline function literal",
                            sig.full_name
                        ),
                    );
                    CallUse::Ordinary
                } else {
                    CallUse::Spawn
                }
            } else {
                CallUse::Ordinary
            };
            self.check_call_args(&name, &name_pos, &sig, call, 0, arg_use);
            self.register_host_error_names(&sig);
            return ExprType {
                base: sig.return_type,
                errorable: sig.errorable,
            };
        }

        if let Expression::Selector(selector) = &call.callee {
            if call_use == CallUse::Spawn {
                self.diag.add(
                    call.callee.pos(),
                    "spawn does not currently support selector or method calls directly; wrap the call in an inline function literal",
                );
            }
            if let Some(result) = self.check_enum_positional_constructor_call(selector, call) {
                return result;
            }
            return self.check_selector_call(selector, call);
        }

        let direct_literal_key = direct_function_literal(&call.callee).map(function_literal_key);
        let callee_type = self.check_expression(&call.callee);
        if callee_type.errorable {
            self.diag
                .add(call.callee.pos(), "call target cannot be errorable");
            return ExprType::invalid();
        }
        if let Some(function_type) = parse_function_type(&callee_type.base) {
            let sig = Signature {
                name: "function value".to_string(),
                package: String::new(),
                full_name: "function value".to_string(),
                method: false,
                receiver: String::new(),
                params: function_type.params,
                return_type: function_type.return_type,
                errorable: function_type.errorable,
                builtin: false,
                host_intrinsic: false,
                exported: false,
            };
            let arg_use = if call_use == CallUse::Spawn && direct_literal_key.is_some() {
                CallUse::Spawn
            } else {
                CallUse::Ordinary
            };
            self.check_call_args(
                &sig.name.clone(),
                &call.callee.pos(),
                &sig,
                call,
                0,
                arg_use,
            );
            if call_use == CallUse::Spawn {
                if let Some(key) = direct_literal_key {
                    self.check_spawn_closure_captures(&key, call.callee.pos());
                } else {
                    self.diag.add(
                        call.callee.pos(),
                        "spawn cannot call an arbitrary function value; use a named function or an inline function literal with share-safe captures",
                    );
                }
            }
            return ExprType {
                base: sig.return_type,
                errorable: sig.errorable,
            };
        }
        self.diag
            .add(call.callee.pos(), "call target must be callable");
        ExprType::invalid()
    }

    fn check_chan_new_call(&mut self, call: &CallExpr) -> Option<ExprType> {
        let Expression::TypeApplication(applied) = &call.callee else {
            return None;
        };
        let Expression::Ident(ident) = &applied.inner else {
            return None;
        };
        if ident.name != "chan_new" {
            return None;
        }
        if applied.type_args.len() != 1 {
            self.diag.add(
                applied.lbracket_pos.clone(),
                format!(
                    "generic builtin {:?} expects 1 type argument, got {}",
                    ident.name,
                    applied.type_args.len()
                ),
            );
            return Some(ExprType::invalid());
        }
        self.require_arg_count(&ident.name, &ident.name_pos, call, 1)?;
        let elem_type = self.resolve_type_ref(&applied.type_args[0]);
        if matches!(elem_type.as_str(), TYPE_VOID | TYPE_NORETURN | TYPE_INVALID)
            || parse_chan_type(&elem_type).is_some()
            || parse_errorable_type(&elem_type).is_some()
        {
            self.diag.add(
                applied.type_args[0].pos.clone(),
                format!(
                    "channel element type {:?} is not allowed",
                    applied.type_args[0].to_string()
                ),
            );
            return Some(ExprType::invalid());
        }
        let mut capacity = self.check_expression(&call.args[0]);
        if capacity.errorable {
            self.diag.add(
                call.args[0].pos(),
                "errorable value cannot be passed as an argument",
            );
            return Some(ExprType::invalid());
        }
        capacity = self.coerce_value(&call.args[0], capacity, TYPE_I32);
        if capacity.base != TYPE_I32 {
            self.diag.add(
                call.args[0].pos(),
                format!(
                    "argument 1 to {:?} must be i32, got {}",
                    ident.name, capacity.base
                ),
            );
            return Some(ExprType::invalid());
        }
        Some(ExprType::plain(format!("chan[{elem_type}]")))
    }

    fn check_taskgroup(&mut self, taskgroup: &TaskgroupExpr) -> ExprType {
        let result_type = self.resolve_type_ref(&taskgroup.result_type);
        let Some(elem_type) = parse_slice_type(&result_type) else {
            self.diag.add(
                taskgroup.result_type.pos.clone(),
                "taskgroup result type must be a slice type",
            );
            return ExprType::invalid();
        };
        let Some(ctx) = self.current.as_mut() else {
            return ExprType::invalid();
        };
        ctx.taskgroups.push(TaskgroupContext {
            result_type: elem_type,
            loop_depth: ctx.loop_depth,
            closure_depth: ctx.closure_depth,
        });
        self.check_block(&taskgroup.body);
        if let Some(ctx) = self.current.as_mut() {
            ctx.taskgroups.pop();
        }
        ExprType::plain(result_type)
    }

    fn check_spawn(&mut self, stmt: &SpawnStmt) {
        let Some(group) = self.current_taskgroup_or_enclosing() else {
            self.diag.add(
                stmt.spawn_pos.clone(),
                "spawn is only valid inside a taskgroup body",
            );
            self.check_expression(&stmt.call);
            return;
        };
        if self.current_taskgroup().is_none() {
            self.diag.add(
                stmt.spawn_pos.clone(),
                "spawn is not allowed inside a function literal within a taskgroup body",
            );
            self.check_expression(&stmt.call);
            return;
        }
        let Expression::Call(call) = &stmt.call else {
            self.diag
                .add(stmt.spawn_pos.clone(), "spawn requires a call expression");
            self.check_expression(&stmt.call);
            return;
        };
        let result = self.check_call(call, CallUse::Spawn);
        if let Some(inner) = parse_errorable_type(&group.result_type) {
            if !result.errorable || result.base != inner {
                self.diag.add(
                    stmt.call.pos(),
                    format!(
                        "spawned call must return {}, got {}",
                        group.result_type,
                        format_expr_type(&result)
                    ),
                );
            }
            return;
        }
        if result.errorable || result.base != group.result_type {
            self.diag.add(
                stmt.call.pos(),
                format!(
                    "spawned call must return {}, got {}",
                    group.result_type,
                    format_expr_type(&result)
                ),
            );
        }
    }

    fn check_spawn_closure_captures(
        &mut self,
        literal_key: &str,
        position: crate::token::Position,
    ) {
        let captures = self
            .info
            .function_literals
            .get(literal_key)
            .map(|info| info.captures.clone())
            .unwrap_or_default();
        for capture in captures {
            self.check_spawn_boundary_type(
                position.clone(),
                format!("spawned closure capture {:?}", capture.name),
                &capture.type_,
            );
        }
    }

    fn check_spawn_boundary_type(
        &mut self,
        position: crate::token::Position,
        boundary: String,
        type_: &str,
    ) {
        let mut visiting = BTreeSet::new();
        let Some(violation) = self.task_share_safety_violation(type_, &mut visiting) else {
            return;
        };
        self.diag.add(
            position,
            format!(
                "{boundary} cannot cross a task boundary: {}",
                violation.describe()
            ),
        );
    }

    fn task_share_safety_violation(
        &self,
        type_: &str,
        visiting: &mut BTreeSet<Type>,
    ) -> Option<TaskShareSafetyViolation> {
        if matches!(
            type_,
            TYPE_BOOL | TYPE_I32 | TYPE_I64 | TYPE_STR | TYPE_ERROR
        ) {
            return None;
        }
        if type_ == TYPE_INVALID {
            return None;
        }
        if let Some(inner) = parse_errorable_type(type_) {
            if inner == TYPE_VOID {
                return None;
            }
            return self
                .task_share_safety_violation(&inner, visiting)
                .map(|violation| violation.prepend("error value"));
        }
        if parse_pointer_type(type_).is_some()
            || parse_slice_type(type_).is_some()
            || parse_map_type(type_).is_some()
            || parse_function_type(type_).is_some()
            || self.info.interfaces.contains_key(type_)
        {
            return Some(TaskShareSafetyViolation::disallowed(type_));
        }
        if let Some(elem_type) = parse_chan_type(type_) {
            return self
                .task_share_safety_violation(&elem_type, visiting)
                .map(|violation| violation.prepend("channel element"));
        }
        if let Some((_, elem_type)) = parse_array_type(type_) {
            return self
                .task_share_safety_violation(&elem_type, visiting)
                .map(|violation| violation.prepend("array element"));
        }
        if let Some(info) = self.info.structs.get(type_) {
            if info.resource {
                return Some(TaskShareSafetyViolation::resource(type_));
            }
            if !visiting.insert(type_.to_string()) {
                return None;
            }
            for field in &info.fields {
                if let Some(violation) = self.task_share_safety_violation(&field.type_, visiting) {
                    visiting.remove(type_);
                    return Some(violation.prepend(format!("field {:?}", field.name)));
                }
            }
            visiting.remove(type_);
            return None;
        }
        if let Some(info) = self.info.enums.get(type_) {
            if !visiting.insert(type_.to_string()) {
                return None;
            }
            for case in &info.cases {
                for field in &case.fields {
                    if let Some(violation) =
                        self.task_share_safety_violation(&field.type_, visiting)
                    {
                        visiting.remove(type_);
                        return Some(
                            violation
                                .prepend(format!("field {:?}", field.name))
                                .prepend(format!("enum case {:?}", case.name)),
                        );
                    }
                }
            }
            visiting.remove(type_);
            return None;
        }

        Some(TaskShareSafetyViolation::disallowed(type_))
    }

    fn check_function_literal(&mut self, literal: &FunctionLiteralExpr) -> ExprType {
        let package = self.current_package().to_owned();
        let mut params = Vec::with_capacity(literal.params.len());
        for param in &literal.params {
            let param_type = self.resolve_type_ref(&param.type_ref);
            if matches!(
                param_type.as_str(),
                TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
            ) {
                self.diag.add(
                    param.type_ref.pos.clone(),
                    format!(
                        "parameter {:?} cannot use type {:?}",
                        param.name,
                        param.type_ref.to_string()
                    ),
                );
                return ExprType::invalid();
            }
            params.push(param_type);
        }

        let return_type = self.resolve_type_ref(&literal.return_type);
        if return_type == TYPE_NORETURN && literal.return_is_bang {
            self.diag.add(
                literal.return_type.pos.clone(),
                "noreturn functions cannot also be errorable",
            );
            return ExprType::invalid();
        }
        if return_type == TYPE_ERROR && literal.return_is_bang {
            self.diag.add(
                literal.return_type.pos.clone(),
                "error functions cannot also be errorable",
            );
            return ExprType::invalid();
        }

        let sig = Signature {
            name: "function value".to_string(),
            package,
            full_name: "function value".to_string(),
            method: false,
            receiver: String::new(),
            params: params.clone(),
            return_type: return_type.clone(),
            errorable: literal.return_is_bang,
            builtin: false,
            host_intrinsic: false,
            exported: false,
        };

        let literal_key = function_literal_key(literal);
        self.info
            .function_literals
            .entry(literal_key.clone())
            .or_insert_with(|| FunctionLiteralInfo {
                signature: sig.clone(),
                captures: Vec::new(),
            });

        let Some(ctx) = self.current.as_mut() else {
            return ExprType::invalid();
        };
        let previous_signature = std::mem::replace(&mut ctx.signature, sig);
        ctx.literal_keys.push(literal_key.clone());
        ctx.closure_depth += 1;
        ctx.scopes.push(BTreeMap::new());
        let literal_depth = ctx.closure_depth;
        for (idx, param) in literal.params.iter().enumerate() {
            let Some(scope) = ctx.scopes.last_mut() else {
                continue;
            };
            if scope.contains_key(&param.name) {
                self.diag.add(
                    param.name_pos.clone(),
                    format!("duplicate parameter {:?}", param.name),
                );
                continue;
            }
            scope.insert(
                param.name.clone(),
                LocalBinding {
                    type_: params[idx].clone(),
                    closure_depth: literal_depth,
                },
            );
        }

        self.check_block(&literal.body);

        let Some(ctx) = self.current.as_mut() else {
            return ExprType::invalid();
        };
        ctx.scopes.pop();
        ctx.closure_depth = ctx.closure_depth.saturating_sub(1);
        let literal_signature = std::mem::replace(&mut ctx.signature, previous_signature);
        ctx.literal_keys.pop();

        if literal_signature.return_type != TYPE_VOID
            && !self.block_definitely_returns(&literal.body)
        {
            if literal_signature.return_type == TYPE_NORETURN {
                self.diag.add(
                    literal.fn_pos.clone(),
                    "function literal must not fall through",
                );
            } else {
                self.diag.add(
                    literal.fn_pos.clone(),
                    "function literal must return a value on all paths",
                );
            }
        }

        if let Some(info) = self.info.function_literals.get_mut(&literal_key) {
            info.signature = literal_signature.clone();
        }

        ExprType::plain(format_function_type(
            &params,
            &return_type,
            literal.return_is_bang,
        ))
    }

    fn check_selector_call(&mut self, selector: &SelectorExpr, call: &CallExpr) -> ExprType {
        let receiver = self.check_expression(&selector.inner);
        if receiver.errorable {
            self.diag.add(
                selector.dot_pos.clone(),
                "method call cannot use an errorable receiver",
            );
            return ExprType::invalid();
        }

        let sig = if let Some(interface) = self.info.interfaces.get(&receiver.base) {
            let Some(method) = interface
                .methods
                .iter()
                .find(|method| method.name == selector.name)
            else {
                self.diag.add(
                    selector.name_pos.clone(),
                    format!(
                        "interface {:?} has no method {:?}",
                        interface.name, selector.name
                    ),
                );
                return ExprType::invalid();
            };
            Signature {
                name: selector.name.clone(),
                package: package_for_type(&receiver.base),
                full_name: format!("{}.{}", receiver.base, selector.name),
                method: true,
                receiver: receiver.base.clone(),
                params: std::iter::once(receiver.base.clone())
                    .chain(method.params.iter().cloned())
                    .collect(),
                return_type: method.return_type.clone(),
                errorable: method.errorable,
                builtin: false,
                host_intrinsic: false,
                exported: true,
            }
        } else {
            let Some(sig) = self.lookup_method(&receiver.base, &selector.name) else {
                self.diag.add(
                    selector.name_pos.clone(),
                    format!(
                        "type {:?} has no method {:?}",
                        diagnostic_type_name(&receiver.base),
                        selector.name
                    ),
                );
                return ExprType::invalid();
            };
            sig
        };

        if sig.package
            != self
                .current
                .as_ref()
                .map(|current| current.signature.package.as_str())
                .unwrap_or_default()
            && !sig.package.is_empty()
            && !sig.exported
        {
            self.diag.add(
                selector.name_pos.clone(),
                format!(
                    "package {:?} does not export method {:?}",
                    package_display_name(&sig.package),
                    selector.name
                ),
            );
            return ExprType::invalid();
        }

        self.check_call_args(
            &selector.name,
            &selector.name_pos,
            &sig,
            call,
            1,
            CallUse::Ordinary,
        );
        ExprType {
            base: sig.return_type,
            errorable: sig.errorable,
        }
    }

    fn check_enum_positional_constructor_call(
        &mut self,
        selector: &SelectorExpr,
        call: &CallExpr,
    ) -> Option<ExprType> {
        let Expression::Ident(ident) = &selector.inner else {
            return None;
        };
        if self.lookup_local(&ident.name).is_some() {
            return None;
        }
        let enum_info = self.info.enums.get(&ident.name)?.clone();
        let enum_case = enum_info
            .cases
            .iter()
            .find(|case| case.name == selector.name)?
            .clone();
        if enum_case.fields.len() != 1 || call.args.len() != 1 {
            return None;
        }

        let field = &enum_case.fields[0];
        let mut arg = self.check_expression(&call.args[0]);
        arg = self.require_non_errorable_value(
            &call.args[0],
            arg,
            "errorable value cannot be used in an enum constructor",
        );
        if arg.base != TYPE_INVALID {
            arg = self.coerce_value(&call.args[0], arg, &field.type_);
            if arg.base != field.type_ {
                self.diag.add(
                    call.args[0].pos(),
                    format!(
                        "cannot assign {} to {}",
                        diagnostic_type_name(&arg.base),
                        diagnostic_type_name(&field.type_)
                    ),
                );
            }
        }
        Some(ExprType::plain(enum_info.name))
    }

    fn check_special_builtin_call(
        &mut self,
        name: &str,
        name_pos: &crate::token::Position,
        call: &CallExpr,
    ) -> Option<ExprType> {
        match name {
            "len" => {
                self.require_arg_count(name, name_pos, call, 1)?;
                let arg = self.check_expression(&call.args[0]);
                if arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                if !is_sequence_type(&arg.base)
                    && parse_map_type(&arg.base).is_none()
                    && arg.base != TYPE_STR
                {
                    self.diag.add(
                        call.args[0].pos(),
                        "len requires an array, slice, map, or str argument",
                    );
                    return Some(ExprType::invalid());
                }
                Some(ExprType::plain(TYPE_I32))
            }
            "to_str" => {
                self.require_arg_count(name, name_pos, call, 1)?;
                let mut arg = self.check_expression(&call.args[0]);
                if arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                if arg.base == TYPE_UNTYPED_INT {
                    arg = self.coerce_value(&call.args[0], arg, TYPE_I32);
                }
                if !matches!(
                    arg.base.as_str(),
                    TYPE_I32 | TYPE_I64 | TYPE_BOOL | TYPE_STR | TYPE_ERROR
                ) {
                    self.diag.add(
                        call.args[0].pos(),
                        "to_str requires an i32, i64, bool, str, or error argument",
                    );
                    return Some(ExprType::invalid());
                }
                Some(ExprType::plain(TYPE_STR))
            }
            "append" => {
                self.require_arg_count(name, name_pos, call, 2)?;
                let slice_arg = self.check_expression(&call.args[0]);
                if slice_arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                let Some(elem_type) = parse_slice_type(&slice_arg.base) else {
                    self.diag.add(
                        call.args[0].pos(),
                        "append requires a slice as its first argument",
                    );
                    return Some(ExprType::invalid());
                };
                let mut value = self.check_expression(&call.args[1]);
                if value.errorable {
                    self.diag.add(
                        call.args[1].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                if matches!(value.base.as_str(), TYPE_VOID | TYPE_NORETURN) {
                    self.diag.add(
                        call.args[1].pos(),
                        format!("argument 2 to {name:?} requires a value"),
                    );
                    return Some(ExprType::invalid());
                }
                value = self.coerce_value(&call.args[1], value, &elem_type);
                if value.base != elem_type {
                    self.diag.add(
                        call.args[1].pos(),
                        format!(
                            "argument 2 to {:?} must be {}, got {}",
                            name,
                            diagnostic_type_name(&elem_type),
                            diagnostic_type_name(&value.base)
                        ),
                    );
                    return Some(ExprType::invalid());
                }
                Some(ExprType::plain(slice_arg.base))
            }
            "has" | "delete" => {
                self.require_arg_count(name, name_pos, call, 2)?;
                let map_arg = self.check_expression(&call.args[0]);
                if map_arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                let Some((key_type, _)) = parse_map_type(&map_arg.base) else {
                    self.diag.add(
                        call.args[0].pos(),
                        format!("{name} requires a map as its first argument"),
                    );
                    return Some(ExprType::invalid());
                };
                let mut key = self.check_expression(&call.args[1]);
                if key.errorable {
                    self.diag.add(
                        call.args[1].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                key = self.coerce_value(&call.args[1], key, &key_type);
                if key.base != key_type {
                    self.diag.add(
                        call.args[1].pos(),
                        format!(
                            "argument 2 to {:?} must be {}, got {}",
                            name,
                            diagnostic_type_name(&key_type),
                            diagnostic_type_name(&key.base)
                        ),
                    );
                    return Some(ExprType::invalid());
                }
                Some(ExprType::plain(if name == "has" {
                    TYPE_BOOL
                } else {
                    TYPE_VOID
                }))
            }
            "keys" => {
                self.require_arg_count(name, name_pos, call, 1)?;
                let map_arg = self.check_expression(&call.args[0]);
                if map_arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                let Some((key_type, _)) = parse_map_type(&map_arg.base) else {
                    self.diag.add(
                        call.args[0].pos(),
                        "keys requires a map as its first argument",
                    );
                    return Some(ExprType::invalid());
                };
                Some(ExprType::plain(format!("[]{key_type}")))
            }
            "chan_send" => {
                self.require_arg_count(name, name_pos, call, 2)?;
                let ch_arg = self.check_expression(&call.args[0]);
                if ch_arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                let Some(elem_type) = parse_chan_type(&ch_arg.base) else {
                    self.diag.add(
                        call.args[0].pos(),
                        "chan_send requires a channel as its first argument",
                    );
                    return Some(ExprType::invalid());
                };
                let mut value = self.check_expression(&call.args[1]);
                if value.errorable {
                    self.diag.add(
                        call.args[1].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                value = self.coerce_value(&call.args[1], value, &elem_type);
                if value.base != elem_type {
                    self.diag.add(
                        call.args[1].pos(),
                        format!(
                            "argument 2 to {:?} must be {}, got {}",
                            name,
                            diagnostic_type_name(&elem_type),
                            diagnostic_type_name(&value.base)
                        ),
                    );
                    return Some(ExprType::invalid());
                }
                self.record_error("Closed");
                Some(ExprType {
                    base: TYPE_VOID.to_string(),
                    errorable: true,
                })
            }
            "chan_recv" => {
                self.require_arg_count(name, name_pos, call, 1)?;
                let ch_arg = self.check_expression(&call.args[0]);
                if ch_arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                let Some(elem_type) = parse_chan_type(&ch_arg.base) else {
                    self.diag.add(
                        call.args[0].pos(),
                        "chan_recv requires a channel as its first argument",
                    );
                    return Some(ExprType::invalid());
                };
                self.record_error("Closed");
                Some(ExprType {
                    base: elem_type,
                    errorable: true,
                })
            }
            "chan_close" => {
                self.require_arg_count(name, name_pos, call, 1)?;
                let ch_arg = self.check_expression(&call.args[0]);
                if ch_arg.errorable {
                    self.diag.add(
                        call.args[0].pos(),
                        "errorable value cannot be passed as an argument",
                    );
                    return Some(ExprType::invalid());
                }
                if parse_chan_type(&ch_arg.base).is_none() {
                    self.diag.add(
                        call.args[0].pos(),
                        "chan_close requires a channel as its first argument",
                    );
                    return Some(ExprType::invalid());
                }
                Some(ExprType::plain(TYPE_VOID))
            }
            _ if builtin_functions().contains_key(name) => None,
            _ => None,
        }
    }

    fn check_call_args(
        &mut self,
        name: &str,
        name_pos: &crate::token::Position,
        sig: &Signature,
        call: &CallExpr,
        arg_offset: usize,
        call_use: CallUse,
    ) {
        let want = sig.params.len().saturating_sub(arg_offset);
        if call.args.len() != want {
            self.diag.add(
                name_pos.clone(),
                format!(
                    "function {:?} expects {want} arguments, got {}",
                    diagnostic_type_name(name),
                    call.args.len()
                ),
            );
        }
        for (idx, arg) in call.args.iter().enumerate() {
            let mut arg_type = self.check_expression(arg);
            if arg_type.errorable {
                self.diag
                    .add(arg.pos(), "errorable value cannot be passed as an argument");
                continue;
            }
            if matches!(arg_type.base.as_str(), TYPE_VOID | TYPE_NORETURN) {
                self.diag.add(
                    arg.pos(),
                    format!("argument {} to {:?} requires a value", idx + 1, name),
                );
                continue;
            }
            let param_idx = idx + arg_offset;
            let Some(param_type) = sig.params.get(param_idx) else {
                continue;
            };
            arg_type = self.coerce_value(arg, arg_type, param_type);
            if arg_type.base == TYPE_INVALID {
                continue;
            }
            if arg_type.base != *param_type {
                self.diag.add(
                    arg.pos(),
                    format!(
                        "argument {} to {:?} must be {}, got {}",
                        idx + 1,
                        diagnostic_type_name(name),
                        diagnostic_type_name(param_type),
                        diagnostic_type_name(&arg_type.base)
                    ),
                );
                continue;
            }
            if call_use == CallUse::Spawn {
                self.check_spawn_boundary_type(
                    arg.pos(),
                    format!("spawn argument {}", idx + 1),
                    param_type,
                );
            }
        }
    }

    fn require_arg_count(
        &mut self,
        name: &str,
        name_pos: &crate::token::Position,
        call: &CallExpr,
        want: usize,
    ) -> Option<()> {
        if call.args.len() != want {
            self.diag.add(
                name_pos.clone(),
                format!(
                    "function {:?} expects {want} arguments, got {}",
                    name,
                    call.args.len()
                ),
            );
            None
        } else {
            Some(())
        }
    }

    fn check_aggregate_element(&mut self, element: &Expression, target: &str, label: &str) {
        let mut value = self.check_expression(element);
        value = self.require_non_errorable_value(
            element,
            value,
            format!("errorable value cannot be used in an {label}"),
        );
        value = self.coerce_value(element, value, target);
        if value.base != target {
            self.diag.add(
                element.pos(),
                format!(
                    "cannot assign {} to {}",
                    diagnostic_type_name(&value.base),
                    diagnostic_type_name(target)
                ),
            );
        }
    }

    fn check_index_like_integer(&mut self, expr: &Expression) -> bool {
        let mut type_ = self.check_expression(expr);
        if type_.errorable {
            self.diag
                .add(expr.pos(), "index expression cannot be errorable");
            return false;
        }
        if !is_integer_type(&type_.base) {
            self.diag
                .add(expr.pos(), "index expression must be an integer");
            return false;
        }
        if type_.base == TYPE_UNTYPED_INT {
            type_ = self.coerce_untyped_integer(expr, type_, TYPE_I32);
            if type_.base == TYPE_UNTYPED_INT {
                self.diag
                    .add(expr.pos(), "index expression must fit in i32");
                return false;
            }
        }
        true
    }

    fn check_slice_bound(&mut self, expr: &Expression) -> bool {
        let mut type_ = self.check_expression(expr);
        if type_.errorable {
            self.diag
                .add(expr.pos(), "slice bounds cannot be errorable");
            return false;
        }
        if !is_integer_type(&type_.base) {
            self.diag.add(expr.pos(), "slice bounds must be integers");
            return false;
        }
        if type_.base == TYPE_UNTYPED_INT {
            type_ = self.coerce_untyped_integer(expr, type_, TYPE_I32);
            if type_.base == TYPE_UNTYPED_INT {
                self.diag.add(expr.pos(), "slice bounds must fit in i32");
                return false;
            }
        }
        true
    }

    fn check_assignment_target(&mut self, target: &Expression) -> Type {
        if let Expression::Ident(expr) = target
            && self.is_captured_local(&expr.name)
        {
            self.diag.add(
                expr.name_pos.clone(),
                format!("cannot assign to captured outer local {:?}", expr.name),
            );
            return TYPE_INVALID.to_string();
        }
        let type_ = self.check_addressable_expr(target, false);
        if type_ == TYPE_INVALID {
            self.diag.add(target.pos(), "invalid assignment target");
        }
        type_
    }

    fn check_map_assignment_target(&mut self, target: &Expression) -> Option<Type> {
        let Expression::Index(index) = target else {
            return None;
        };
        let inner = self.check_expression(&index.inner);
        if inner.errorable {
            self.diag.add(
                index.lbracket_pos.clone(),
                "indexing cannot use an errorable value",
            );
            return Some(TYPE_INVALID.to_string());
        }
        let (key_type, value_type) = parse_map_type(&inner.base)?;
        let mut key = self.check_expression(&index.index);
        if key.errorable {
            self.diag
                .add(index.index.pos(), "map key expression cannot be errorable");
            return Some(TYPE_INVALID.to_string());
        }
        key = self.coerce_value(&index.index, key, &key_type);
        if key.base != key_type {
            self.diag.add(
                index.index.pos(),
                format!(
                    "map key must be {}, got {}",
                    diagnostic_type_name(&key_type),
                    diagnostic_type_name(&key.base)
                ),
            );
            return Some(TYPE_INVALID.to_string());
        }
        Some(value_type)
    }

    fn check_addressable_expr(&mut self, expr: &Expression, allow_composite_literal: bool) -> Type {
        match expr {
            Expression::Ident(expr) => match self.lookup_local(&expr.name) {
                Some(binding) => {
                    if self.is_captured_local(&expr.name) {
                        self.diag.add(
                            expr.name_pos.clone(),
                            format!("captured outer local {:?} is not addressable", expr.name),
                        );
                        return TYPE_INVALID.to_string();
                    }
                    binding.type_
                }
                None => {
                    self.diag.add(
                        expr.name_pos.clone(),
                        format!("unknown local {:?}", expr.name),
                    );
                    TYPE_INVALID.to_string()
                }
            },
            Expression::Group(expr) => {
                self.check_addressable_expr(&expr.inner, allow_composite_literal)
            }
            Expression::Selector(expr) => {
                let base = self.check_addressable_expr(&expr.inner, false);
                let resolved = parse_pointer_type(&base).unwrap_or(base);
                let Some(info) = self.info.structs.get(&resolved).cloned() else {
                    self.diag
                        .add(expr.dot_pos.clone(), "field access requires a struct value");
                    return TYPE_INVALID.to_string();
                };
                match info
                    .fields
                    .iter()
                    .find(|field| field.name == expr.name)
                    .cloned()
                {
                    Some(field) => {
                        if self.private_field_is_inaccessible(&info, &field, &expr.name_pos) {
                            TYPE_INVALID.to_string()
                        } else {
                            field.type_
                        }
                    }
                    None => {
                        self.diag.add(
                            expr.name_pos.clone(),
                            format!(
                                "struct {:?} has no field {:?}",
                                diagnostic_type_name(&info.name),
                                expr.name
                            ),
                        );
                        TYPE_INVALID.to_string()
                    }
                }
            }
            Expression::Index(expr) => {
                let base = self.check_addressable_expr(&expr.inner, false);
                let Some(elem_type) = sequence_element_type(&base) else {
                    self.diag.add(
                        expr.lbracket_pos.clone(),
                        "indexing requires an array or slice value",
                    );
                    return TYPE_INVALID.to_string();
                };
                if !self.check_index_like_integer(&expr.index) {
                    return TYPE_INVALID.to_string();
                }
                elem_type
            }
            Expression::Unary(expr) if expr.operator == Kind::Star => {
                let inner = self.check_expression(&expr.inner);
                parse_pointer_type(&inner.base).unwrap_or_else(|| {
                    self.diag.add(
                        expr.op_pos.clone(),
                        "dereference requires a pointer operand",
                    );
                    TYPE_INVALID.to_string()
                })
            }
            Expression::StructLiteral(expr) if allow_composite_literal => {
                self.check_struct_literal(expr).base
            }
            Expression::ArrayLiteral(expr) if allow_composite_literal => {
                self.check_array_literal(expr).base
            }
            Expression::SliceLiteral(expr) if allow_composite_literal => {
                self.check_slice_literal(expr).base
            }
            Expression::MapLiteral(expr) if allow_composite_literal => {
                self.check_map_literal(expr).base
            }
            _ => TYPE_INVALID.to_string(),
        }
    }

    fn require_non_errorable_value(
        &mut self,
        expr: &Expression,
        expr_type: ExprType,
        message: impl Into<String>,
    ) -> ExprType {
        if expr_type.errorable {
            self.diag.add(expr.pos(), message.into());
            return ExprType::invalid();
        }
        if expr_type.base == TYPE_INVALID {
            return ExprType::invalid();
        }
        if matches!(expr_type.base.as_str(), TYPE_VOID | TYPE_NORETURN) {
            self.diag.add(expr.pos(), "declaration requires a value");
            return ExprType::invalid();
        }
        expr_type
    }

    fn coerce_value(&mut self, expr: &Expression, expr_type: ExprType, target: &str) -> ExprType {
        if expr_type.base == TYPE_NIL && is_pointer_type(target) {
            return ExprType::plain(target.to_string());
        }
        if self.info.interfaces.contains_key(target) {
            if expr_type.base == target {
                return ExprType::plain(target.to_string());
            }
            if self.info.interfaces.contains_key(&expr_type.base) {
                return expr_type;
            }
            match self.satisfies_interface(&expr_type.base, target) {
                Ok(true) => return ExprType::plain(target.to_string()),
                Ok(false) => {}
                Err(message) => {
                    self.diag.add(expr.pos(), message);
                    return ExprType::invalid();
                }
            }
        }
        self.coerce_untyped_integer(expr, expr_type, target)
    }

    fn coerce_untyped_integer(
        &mut self,
        expr: &Expression,
        expr_type: ExprType,
        target: &str,
    ) -> ExprType {
        if expr_type.base != TYPE_UNTYPED_INT || !matches!(target, TYPE_I32 | TYPE_I64) {
            return expr_type;
        }
        if self.int_expression_fits(expr, target) {
            ExprType::plain(target.to_string())
        } else {
            expr_type
        }
    }

    fn coerce_binary_integers(
        &mut self,
        left_expr: &Expression,
        left: ExprType,
        right_expr: &Expression,
        right: ExprType,
    ) -> Option<Type> {
        if !is_integer_type(&left.base) || !is_integer_type(&right.base) {
            return None;
        }
        match (left.base.as_str(), right.base.as_str()) {
            (TYPE_UNTYPED_INT, TYPE_UNTYPED_INT) => Some(TYPE_UNTYPED_INT.to_string()),
            (TYPE_UNTYPED_INT, _) if self.int_expression_fits(left_expr, &right.base) => {
                Some(right.base)
            }
            (_, TYPE_UNTYPED_INT) if self.int_expression_fits(right_expr, &left.base) => {
                Some(left.base)
            }
            _ if left.base == right.base => Some(left.base),
            _ => None,
        }
    }

    fn same_comparable_type(&self, left: &str, right: &str) -> bool {
        (left == right && left != TYPE_NIL)
            || (left == TYPE_NIL && is_pointer_type(right))
            || (right == TYPE_NIL && is_pointer_type(left))
    }

    fn int_expression_fits(&self, expr: &Expression, target: &str) -> bool {
        match const_integer_expression(expr) {
            Some(value) => int_value_fits(value, target),
            None => int_expression_operands_fit(expr, target),
        }
    }

    fn default_untyped_integer_type(&self, expr: &Expression) -> Type {
        infer_untyped_integer_type(expr)
            .unwrap_or(TYPE_INVALID)
            .to_string()
    }

    fn block_definitely_returns(&self, block: &BlockStmt) -> bool {
        block
            .stmts
            .iter()
            .any(|stmt| self.stmt_definitely_returns(stmt))
    }

    fn block_terminates_control_flow(&self, block: &BlockStmt) -> bool {
        block
            .stmts
            .iter()
            .any(|stmt| self.stmt_terminates_control_flow(stmt))
    }

    fn stmt_terminates_control_flow(&self, stmt: &Statement) -> bool {
        match stmt {
            Statement::Return(_) | Statement::Break(_) | Statement::Continue(_) => true,
            Statement::Block(block) => self.block_terminates_control_flow(block),
            Statement::Expr(stmt) => self.expression_definitely_returns(&stmt.expr),
            Statement::If(stmt) => {
                stmt.else_stmt.is_some()
                    && self.block_terminates_control_flow(&stmt.then_block)
                    && stmt
                        .else_stmt
                        .as_ref()
                        .is_some_and(|else_stmt| self.stmt_terminates_control_flow(else_stmt))
            }
            Statement::Match(stmt) => {
                if stmt.arms.is_empty() && stmt.else_body.is_none() {
                    return false;
                }
                if stmt
                    .arms
                    .iter()
                    .any(|arm| !self.block_terminates_control_flow(&arm.body))
                {
                    return false;
                }
                stmt.else_body
                    .as_ref()
                    .is_none_or(|else_body| self.block_terminates_control_flow(else_body))
            }
            _ => false,
        }
    }

    fn stmt_definitely_returns(&self, stmt: &Statement) -> bool {
        match stmt {
            Statement::Return(_) => true,
            Statement::Block(block) => self.block_definitely_returns(block),
            Statement::Expr(stmt) => self.expression_definitely_returns(&stmt.expr),
            Statement::If(stmt) => {
                stmt.else_stmt.is_some()
                    && self.block_definitely_returns(&stmt.then_block)
                    && stmt
                        .else_stmt
                        .as_ref()
                        .is_some_and(|else_stmt| self.stmt_definitely_returns(else_stmt))
            }
            Statement::Match(stmt) => {
                if stmt.arms.is_empty() && stmt.else_body.is_none() {
                    return false;
                }
                if stmt
                    .arms
                    .iter()
                    .any(|arm| !self.block_definitely_returns(&arm.body))
                {
                    return false;
                }
                stmt.else_body
                    .as_ref()
                    .is_none_or(|else_body| self.block_definitely_returns(else_body))
            }
            _ => false,
        }
    }

    fn expression_definitely_returns(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Group(expr) => self.expression_definitely_returns(&expr.inner),
            Expression::Call(call) => call_name(&call.callee)
                .and_then(|(name, _)| self.functions.get(&name))
                .is_some_and(|sig| sig.return_type == TYPE_NORETURN),
            _ => false,
        }
    }

    fn push_scope(&mut self) {
        if let Some(ctx) = &mut self.current {
            ctx.scopes.push(BTreeMap::new());
        }
    }

    fn enter_loop(&mut self) {
        if let Some(ctx) = &mut self.current {
            ctx.loop_depth += 1;
        }
    }

    fn exit_loop(&mut self) {
        if let Some(ctx) = &mut self.current {
            ctx.loop_depth = ctx.loop_depth.saturating_sub(1);
        }
    }

    fn check_block_in_current_scope(&mut self, block: &BlockStmt) {
        for stmt in &block.stmts {
            self.check_statement(stmt);
        }
    }

    fn pop_scope(&mut self) {
        if let Some(ctx) = &mut self.current {
            ctx.scopes.pop();
        }
    }

    fn bind_local(&mut self, name: &str, type_: Type) {
        if let Some(ctx) = self.current.as_mut()
            && let Some(scope) = ctx.scopes.last_mut()
        {
            scope.insert(
                name.to_string(),
                LocalBinding {
                    type_,
                    closure_depth: ctx.closure_depth,
                },
            );
        }
    }

    fn scope_owns(&self, name: &str) -> bool {
        self.current
            .as_ref()
            .and_then(|ctx| ctx.scopes.last())
            .is_some_and(|scope| scope.contains_key(name))
    }

    fn lookup_local(&self, name: &str) -> Option<LocalBinding> {
        self.current.as_ref().and_then(|ctx| {
            ctx.scopes
                .iter()
                .rev()
                .find_map(|scope| scope.get(name).cloned())
        })
    }

    fn is_captured_local(&self, name: &str) -> bool {
        let Some(ctx) = self.current.as_ref() else {
            return false;
        };
        let Some(binding) = self.lookup_local(name) else {
            return false;
        };
        binding.closure_depth < ctx.closure_depth
    }

    fn address_root_captured_outer_local(&mut self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Ident(expr) => {
                let binding = self.lookup_local(&expr.name)?;
                if !self.is_captured_local(&expr.name) {
                    return None;
                }
                self.record_capture(&expr.name, &binding.type_, binding.closure_depth);
                Some(expr.name.clone())
            }
            Expression::Group(expr) => self.address_root_captured_outer_local(&expr.inner),
            Expression::Selector(expr) => self.address_root_captured_outer_local(&expr.inner),
            Expression::Index(expr) => self.address_root_captured_outer_local(&expr.inner),
            _ => None,
        }
    }

    fn current_taskgroup(&self) -> Option<TaskgroupContext> {
        self.current.as_ref().and_then(|ctx| {
            ctx.taskgroups
                .iter()
                .rev()
                .find(|group| group.closure_depth == ctx.closure_depth)
                .cloned()
        })
    }

    fn current_taskgroup_or_enclosing(&self) -> Option<TaskgroupContext> {
        self.current
            .as_ref()
            .and_then(|ctx| ctx.taskgroups.last().cloned())
    }

    fn lookup_method(&self, receiver: &str, name: &str) -> Option<Signature> {
        self.info
            .methods
            .get(receiver)
            .and_then(|methods| methods.get(name))
            .cloned()
    }

    fn enum_type_for_payload(&self, payload_type: &str) -> Option<Type> {
        self.info.enums.iter().find_map(|(enum_name, enum_info)| {
            enum_info
                .cases
                .iter()
                .any(|case| case.payload_type == payload_type)
                .then(|| enum_name.clone())
        })
    }

    fn satisfies_interface(&self, concrete: &str, interface: &str) -> Result<bool, String> {
        if concrete == interface {
            return Ok(true);
        }
        if concrete == TYPE_INVALID || interface == TYPE_INVALID {
            return Ok(false);
        }
        if self.info.interfaces.contains_key(concrete) {
            return Err(format!(
                "cannot assign {} to {}",
                diagnostic_type_name(concrete),
                diagnostic_type_name(interface)
            ));
        }
        let Some(info) = self.info.interfaces.get(interface) else {
            return Ok(false);
        };
        for method in &info.methods {
            let Some(sig) = self.lookup_method(concrete, &method.name) else {
                return Err(format!(
                    "type {:?} does not satisfy interface {:?} (missing method {:?})",
                    diagnostic_type_name(concrete),
                    diagnostic_type_name(interface),
                    method.name
                ));
            };
            if sig.return_type != method.return_type
                || sig.errorable != method.errorable
                || sig.params.len() != method.params.len() + 1
                || sig.params.iter().skip(1).ne(method.params.iter())
            {
                return Err(format!(
                    "method {:?} on {:?} does not match interface {:?}",
                    method.name,
                    diagnostic_type_name(concrete),
                    diagnostic_type_name(interface)
                ));
            }
        }
        Ok(true)
    }

    fn current_can_propagate_error(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|ctx| ctx.signature.errorable || ctx.signature.return_type == TYPE_ERROR)
    }

    fn record_capture(&mut self, name: &str, type_: &str, binding_depth: usize) {
        let keys = self
            .current
            .as_ref()
            .map(|ctx| {
                ctx.literal_keys
                    .iter()
                    .enumerate()
                    .filter(|(idx, _)| idx + 1 > binding_depth)
                    .map(|(_, key)| key.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for key in keys {
            let Some(info) = self.info.function_literals.get_mut(&key) else {
                continue;
            };
            if info.captures.iter().any(|capture| capture.name == name) {
                continue;
            }
            info.captures.push(CaptureInfo {
                name: name.to_string(),
                type_: type_.to_string(),
            });
        }
    }

    fn record_error(&mut self, name: &str) {
        self.info.error_codes.entry(name.to_string()).or_insert(0);
    }

    fn register_host_error_names(&mut self, signature: &Signature) {
        if !signature.host_intrinsic {
            return;
        }
        if is_fs_host_intrinsic(&signature.full_name) {
            for name in [
                "AlreadyExists",
                "Closed",
                "IO",
                "InvalidArgument",
                "InvalidPath",
                "NotFound",
                "PermissionDenied",
            ] {
                self.record_error(name);
            }
            return;
        }

        if is_process_host_intrinsic(&signature.full_name) {
            for name in [
                "IO",
                "InvalidArgument",
                "LimitExceeded",
                "NotFound",
                "PermissionDenied",
                "Timeout",
                "Cancelled",
            ] {
                self.record_error(name);
            }
            return;
        }

        if is_env_host_intrinsic(&signature.full_name) {
            for name in ["IO", "InvalidArgument", "NotFound", "PermissionDenied"] {
                self.record_error(name);
            }
            return;
        }

        if is_net_host_intrinsic(&signature.full_name) {
            for name in [
                "AddrInUse",
                "Closed",
                "ConnectionRefused",
                "ConnectionReset",
                "IO",
                "InvalidArgument",
                "NotFound",
                "PermissionDenied",
                "Timeout",
            ] {
                self.record_error(name);
            }
        }
    }

    fn assign_error_codes(&mut self) {
        let mut ordered = self.info.error_codes.keys().cloned().collect::<Vec<_>>();
        ordered.sort();
        for (idx, name) in ordered.iter().enumerate() {
            self.info.error_codes.insert(name.clone(), (idx + 1) as i32);
        }
        self.info.ordered_errors = ordered;
    }

    fn resolve_type_ref(&mut self, ref_: &TypeRef) -> Type {
        match ref_.kind {
            TypeRefKind::Errorable => {
                let Some(elem) = ref_.elem.as_deref() else {
                    self.diag.add(
                        ref_.pos.clone(),
                        "errorable type is missing an element type",
                    );
                    return TYPE_INVALID.to_string();
                };
                let elem_type = self.resolve_type_ref(elem);
                if matches!(elem_type.as_str(), TYPE_NORETURN | TYPE_INVALID) {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!("errorable value type {:?} is not allowed", elem.to_string()),
                    );
                    return TYPE_INVALID.to_string();
                }
                if elem_type.starts_with('!') {
                    self.diag.add(
                        ref_.pos.clone(),
                        "nested errorable value types are not allowed",
                    );
                    return TYPE_INVALID.to_string();
                }
                format!("!{elem_type}")
            }
            TypeRefKind::Pointer => {
                let Some(elem) = ref_.elem.as_deref() else {
                    self.diag
                        .add(ref_.pos.clone(), "pointer type is missing an element type");
                    return TYPE_INVALID.to_string();
                };
                let elem_type = self.resolve_type_ref(elem);
                if matches!(elem_type.as_str(), TYPE_VOID | TYPE_NORETURN | TYPE_INVALID) {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!("pointer target type {:?} is not allowed", elem.to_string()),
                    );
                    return TYPE_INVALID.to_string();
                }
                format!("*{elem_type}")
            }
            TypeRefKind::Slice => {
                let Some(elem) = ref_.elem.as_deref() else {
                    self.diag
                        .add(ref_.pos.clone(), "slice type is missing an element type");
                    return TYPE_INVALID.to_string();
                };
                let elem_type = self.resolve_type_ref(elem);
                if matches!(elem_type.as_str(), TYPE_NORETURN | TYPE_INVALID) {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!("slice element type {:?} is not allowed", elem.to_string()),
                    );
                    return TYPE_INVALID.to_string();
                }
                format!("[]{elem_type}")
            }
            TypeRefKind::Array => {
                if ref_.array_len < 0 || ref_.array_len > i32::MAX.into() {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!("array length {} is out of range", ref_.array_len),
                    );
                    return TYPE_INVALID.to_string();
                }
                let Some(elem) = ref_.elem.as_deref() else {
                    self.diag
                        .add(ref_.pos.clone(), "array type is missing an element type");
                    return TYPE_INVALID.to_string();
                };
                let elem_type = self.resolve_type_ref(elem);
                if matches!(elem_type.as_str(), TYPE_VOID | TYPE_NORETURN | TYPE_INVALID) {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!("array element type {:?} is not allowed", elem.to_string()),
                    );
                    return TYPE_INVALID.to_string();
                }
                format!("[{}]{elem_type}", ref_.array_len)
            }
            TypeRefKind::Map => {
                let (Some(key), Some(value)) = (ref_.key.as_deref(), ref_.value.as_deref()) else {
                    self.diag.add(ref_.pos.clone(), "map type is incomplete");
                    return TYPE_INVALID.to_string();
                };
                let key_type = self.resolve_type_ref(key);
                if !is_map_key_type(&key_type) {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!(
                            "map key type {:?} is not supported; must be bool, i32, i64, or str",
                            key.to_string()
                        ),
                    );
                    return TYPE_INVALID.to_string();
                }
                let value_type = self.resolve_type_ref(value);
                if matches!(
                    value_type.as_str(),
                    TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
                ) {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!("map value type {:?} is not allowed", value.to_string()),
                    );
                    return TYPE_INVALID.to_string();
                }
                format!("map[{key_type}]{value_type}")
            }
            TypeRefKind::Chan => {
                let Some(elem) = ref_.elem.as_deref() else {
                    self.diag
                        .add(ref_.pos.clone(), "channel type is missing an element type");
                    return TYPE_INVALID.to_string();
                };
                let elem_type = self.resolve_type_ref(elem);
                if matches!(elem_type.as_str(), TYPE_VOID | TYPE_NORETURN | TYPE_INVALID)
                    || elem_type.starts_with("chan[")
                    || elem_type.starts_with('!')
                {
                    self.diag.add(
                        ref_.pos.clone(),
                        format!("channel element type {:?} is not allowed", elem.to_string()),
                    );
                    return TYPE_INVALID.to_string();
                }
                format!("chan[{elem_type}]")
            }
            TypeRefKind::Function => {
                let mut params = Vec::with_capacity(ref_.params.len());
                for param in &ref_.params {
                    let param_type = self.resolve_type_ref(param);
                    if matches!(
                        param_type.as_str(),
                        TYPE_VOID | TYPE_NORETURN | TYPE_INVALID
                    ) {
                        self.diag.add(
                            param.pos.clone(),
                            format!(
                                "function parameter type {:?} is not allowed",
                                param.to_string()
                            ),
                        );
                        return TYPE_INVALID.to_string();
                    }
                    params.push(param_type);
                }
                let Some(return_type) = ref_.return_type.as_deref() else {
                    self.diag
                        .add(ref_.pos.clone(), "function type is missing a return type");
                    return TYPE_INVALID.to_string();
                };
                let ret_type = self.resolve_type_ref(return_type);
                if ret_type == TYPE_NORETURN && ref_.errorable {
                    self.diag.add(
                        ref_.pos.clone(),
                        "noreturn functions cannot also be errorable",
                    );
                    return TYPE_INVALID.to_string();
                }
                if ret_type == TYPE_ERROR && ref_.errorable {
                    self.diag
                        .add(ref_.pos.clone(), "error functions cannot also be errorable");
                    return TYPE_INVALID.to_string();
                }
                let bang = if ref_.errorable { "!" } else { "" };
                format!("fn({}) {bang}{ret_type}", params.join(", "))
            }
            TypeRefKind::Named => self.resolve_named_type_ref(ref_),
        }
    }

    fn resolve_named_type_ref(&mut self, ref_: &TypeRef) -> Type {
        if !ref_.type_args.is_empty() {
            self.diag.add(
                ref_.pos.clone(),
                format!(
                    "generic type {:?} must be monomorphized before checking",
                    ref_.to_string()
                ),
            );
            return TYPE_INVALID.to_string();
        }

        if is_builtin_type(&ref_.name)
            || self.struct_decls.contains_key(&ref_.name)
            || self.info.structs.contains_key(&ref_.name)
            || self.interface_decls.contains_key(&ref_.name)
            || self.enum_decls.contains_key(&ref_.name)
        {
            return ref_.name.clone();
        }

        self.diag.add(
            ref_.pos.clone(),
            format!("unknown type {:?}", ref_.to_string()),
        );
        TYPE_INVALID.to_string()
    }

    fn resolve_method_receiver_type(&mut self, ref_: &TypeRef) -> Type {
        let type_ = self.resolve_type_ref(ref_);
        if type_ == TYPE_INVALID {
            return TYPE_INVALID.to_string();
        }
        let base = method_receiver_base_type(&type_);
        if self.info.structs.contains_key(&base) {
            return type_;
        }
        self.diag.add(
            ref_.pos.clone(),
            "method receiver must be a named struct type or pointer to one",
        );
        TYPE_INVALID.to_string()
    }

    fn lookup_method_for_base(&self, base: &str, name: &str) -> bool {
        self.info.methods.iter().any(|(receiver, methods)| {
            method_receiver_base_type(receiver) == base && methods.contains_key(name)
        })
    }

    fn check_type_cycles(&mut self, program: &Program) {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();

        for decl in &program.structs {
            self.visit_type_for_cycles(&decl.name, &mut visiting, &mut visited);
        }
        for decl in &program.enums {
            self.visit_type_for_cycles(&decl.name, &mut visiting, &mut visited);
        }
    }

    fn visit_type_for_cycles(
        &mut self,
        name: &str,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) {
        if visited.contains(name) {
            return;
        }
        if visiting.contains(name) {
            if let Some(decl) = self.struct_decls.get(name) {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!(
                        "struct {:?} cannot contain itself recursively",
                        diagnostic_type_name(name)
                    ),
                );
            } else if let Some(decl) = self.enum_decls.get(name) {
                self.diag.add(
                    decl.name_pos.clone(),
                    format!(
                        "enum {:?} cannot contain itself recursively",
                        diagnostic_type_name(name)
                    ),
                );
            }
            return;
        }

        visiting.insert(name.to_string());
        for dep in self.value_type_dependencies(name) {
            self.visit_type_for_cycles(&dep, visiting, visited);
        }
        visiting.remove(name);
        visited.insert(name.to_string());
    }

    fn value_type_dependencies(&self, name: &str) -> Vec<String> {
        if let Some(decl) = self.struct_decls.get(name) {
            return decl
                .fields
                .iter()
                .flat_map(|field| self.type_ref_value_dependencies(&field.type_ref))
                .collect();
        }
        if let Some(decl) = self.enum_decls.get(name) {
            return decl
                .cases
                .iter()
                .flat_map(|case| {
                    case.fields
                        .iter()
                        .flat_map(|field| self.type_ref_value_dependencies(&field.type_ref))
                        .collect::<Vec<_>>()
                })
                .collect();
        }
        Vec::new()
    }

    fn type_ref_value_dependencies(&self, ref_: &TypeRef) -> Vec<String> {
        match ref_.kind {
            TypeRefKind::Errorable | TypeRefKind::Array => ref_
                .elem
                .as_deref()
                .map(|elem| self.type_ref_value_dependencies(elem))
                .unwrap_or_default(),
            TypeRefKind::Named => {
                if self.struct_decls.contains_key(&ref_.name)
                    || self.enum_decls.contains_key(&ref_.name)
                {
                    vec![ref_.name.clone()]
                } else {
                    Vec::new()
                }
            }
            TypeRefKind::Pointer
            | TypeRefKind::Slice
            | TypeRefKind::Map
            | TypeRefKind::Chan
            | TypeRefKind::Function => Vec::new(),
        }
    }
}

fn builtin_functions() -> BTreeMap<String, Signature> {
    [
        ("print", vec![TYPE_STR], TYPE_VOID, false),
        ("panic", vec![TYPE_STR], TYPE_NORETURN, false),
        ("len", vec![TYPE_INVALID], TYPE_I32, false),
        (
            "append",
            vec![TYPE_INVALID, TYPE_INVALID],
            TYPE_INVALID,
            false,
        ),
        ("has", vec![TYPE_INVALID, TYPE_INVALID], TYPE_BOOL, false),
        ("delete", vec![TYPE_INVALID, TYPE_INVALID], TYPE_VOID, false),
        ("keys", vec![TYPE_INVALID], TYPE_INVALID, false),
        ("chr", vec![TYPE_I32], TYPE_STR, false),
        ("i32_to_i64", vec![TYPE_I32], TYPE_I64, false),
        ("i64_to_i32", vec![TYPE_I64], TYPE_I32, false),
        ("sb_new", vec![], TYPE_I64, false),
        ("sb_write", vec![TYPE_I64, TYPE_STR], TYPE_VOID, false),
        ("sb_string", vec![TYPE_I64], TYPE_STR, false),
        ("chan_new", vec![TYPE_I32], TYPE_INVALID, false),
        (
            "chan_send",
            vec![TYPE_INVALID, TYPE_INVALID],
            TYPE_VOID,
            true,
        ),
        ("chan_recv", vec![TYPE_INVALID], TYPE_INVALID, true),
        ("chan_close", vec![TYPE_INVALID], TYPE_VOID, false),
        ("to_str", vec![TYPE_INVALID], TYPE_STR, false),
    ]
    .into_iter()
    .map(|(name, params, return_type, errorable)| {
        (
            name.to_string(),
            Signature {
                name: name.to_string(),
                package: String::new(),
                full_name: name.to_string(),
                method: false,
                receiver: String::new(),
                params: params.into_iter().map(ToString::to_string).collect(),
                return_type: return_type.to_string(),
                errorable,
                builtin: true,
                host_intrinsic: false,
                exported: false,
            },
        )
    })
    .collect()
}

pub(crate) fn is_builtin_function(name: &str) -> bool {
    builtin_functions().contains_key(name)
}

fn is_fs_host_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "fs.read_file"
            | "fs.write_file"
            | "fs.read_dir"
            | "fs.stat"
            | "fs.mkdir_all"
            | "fs.remove_all"
            | "fs.temp_dir"
            | "fs.open_read_handle"
            | "fs.open_write_handle"
            | "fs.read_handle"
            | "fs.write_handle"
            | "fs.close_handle"
    )
}

fn is_process_host_intrinsic(name: &str) -> bool {
    matches!(name, "process.run" | "process.run_inherit")
}

fn is_env_host_intrinsic(name: &str) -> bool {
    name == "env.lookup"
}

fn is_net_host_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "net.listen"
            | "net.accept"
            | "net.listener_addr"
            | "net.close_listener"
            | "net.connect"
            | "net.read"
            | "net.write"
            | "net.close"
            | "net.local_addr"
            | "net.remote_addr"
            | "net.set_read_deadline"
            | "net.set_write_deadline"
            | "net.resolve"
    )
}

fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        TYPE_VOID | TYPE_NORETURN | TYPE_BOOL | TYPE_I32 | TYPE_I64 | TYPE_STR | TYPE_ERROR
    )
}

fn is_map_key_type(type_: &str) -> bool {
    matches!(type_, TYPE_BOOL | TYPE_I32 | TYPE_I64 | TYPE_STR)
}

fn is_integer_type(type_: &str) -> bool {
    matches!(type_, TYPE_I32 | TYPE_I64 | TYPE_UNTYPED_INT)
}

fn is_pointer_type(type_: &str) -> bool {
    parse_pointer_type(type_).is_some()
}

fn parse_pointer_type(type_: &str) -> Option<Type> {
    type_.strip_prefix('*').map(ToString::to_string)
}

fn parse_errorable_type(type_: &str) -> Option<Type> {
    type_.strip_prefix('!').map(ToString::to_string)
}

fn parse_slice_type(type_: &str) -> Option<Type> {
    type_.strip_prefix("[]").map(ToString::to_string)
}

fn parse_chan_type(type_: &str) -> Option<Type> {
    let elem = type_.strip_prefix("chan[")?.strip_suffix(']')?;
    if elem.is_empty() {
        return None;
    }
    Some(elem.to_string())
}

fn parse_array_type(type_: &str) -> Option<(usize, Type)> {
    let rest = type_.strip_prefix('[')?;
    let end = rest.find(']')?;
    let len = rest[..end].parse::<usize>().ok()?;
    let elem = rest[end + 1..].to_string();
    if elem.is_empty() {
        return None;
    }
    Some((len, elem))
}

fn parse_map_type(type_: &str) -> Option<(Type, Type)> {
    let rest = type_.strip_prefix("map[")?;
    let mut depth = 0usize;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' if depth == 0 => {
                let key = rest[..idx].to_string();
                let value = rest[idx + 1..].to_string();
                if key.is_empty() || value.is_empty() {
                    return None;
                }
                return Some((key, value));
            }
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FunctionType {
    params: Vec<Type>,
    return_type: Type,
    errorable: bool,
}

fn parse_function_type(type_: &str) -> Option<FunctionType> {
    let rest = type_.strip_prefix("fn(")?;
    let close = find_matching_paren(rest)?;
    let params_text = &rest[..close];
    let mut return_text = rest[close + 1..].trim_start();
    if return_text.is_empty() {
        return None;
    }
    let errorable = return_text.starts_with('!');
    if errorable {
        return_text = &return_text[1..];
    }
    if return_text.is_empty() {
        return None;
    }
    Some(FunctionType {
        params: split_top_level_types(params_text)?,
        return_type: return_text.to_string(),
        errorable,
    })
}

fn format_function_type(params: &[Type], return_type: &str, errorable: bool) -> Type {
    let bang = if errorable { "!" } else { "" };
    format!("fn({}) {bang}{return_type}", params.join(", "))
}

pub(crate) fn function_literal_key(literal: &FunctionLiteralExpr) -> String {
    let source_key = format!(
        "{}:{}:{}",
        literal.fn_pos.file, literal.fn_pos.line, literal.fn_pos.column
    );
    if literal.enclosing_function.is_empty() {
        source_key
    } else {
        format!("{}@{source_key}", literal.enclosing_function)
    }
}

fn format_expr_type(expr_type: &ExprType) -> Type {
    if expr_type.errorable {
        format!("!{}", diagnostic_type_name(&expr_type.base))
    } else {
        diagnostic_type_name(&expr_type.base)
    }
}

fn diagnostic_type_name(type_: &str) -> String {
    const ENTRY_PACKAGE_PREFIX: &str = "main.";

    let mut out = String::with_capacity(type_.len());
    let mut chars = type_.char_indices();
    let mut previous = None;

    while let Some((idx, ch)) = chars.next() {
        if type_[idx..].starts_with(ENTRY_PACKAGE_PREFIX) && is_type_name_boundary(previous) {
            for _ in 1..ENTRY_PACKAGE_PREFIX.len() {
                chars.next();
            }
            previous = Some('.');
            continue;
        }
        out.push(ch);
        previous = Some(ch);
    }

    out
}

fn is_type_name_boundary(ch: Option<char>) -> bool {
    ch.is_none_or(|ch| matches!(ch, '*' | '!' | '[' | ']' | '(' | ')' | ',' | ' '))
}

fn find_matching_paren(text: &str) -> Option<usize> {
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    for (idx, ch) in text.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' if bracket_depth > 0 => bracket_depth -= 1,
            '(' => paren_depth += 1,
            ')' if bracket_depth == 0 && paren_depth == 0 => return Some(idx),
            ')' if paren_depth > 0 => paren_depth -= 1,
            _ => {}
        }
    }
    None
}

fn split_top_level_types(text: &str) -> Option<Vec<Type>> {
    if text.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut out = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    for (idx, ch) in text.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' if bracket_depth > 0 => bracket_depth -= 1,
            '(' => paren_depth += 1,
            ')' if paren_depth > 0 => paren_depth -= 1,
            ',' if bracket_depth == 0 && paren_depth == 0 => {
                let part = text[start..idx].trim();
                if part.is_empty() {
                    return None;
                }
                out.push(part.to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let part = text[start..].trim();
    if part.is_empty() {
        return None;
    }
    out.push(part.to_string());
    Some(out)
}

fn sequence_element_type(type_: &str) -> Option<Type> {
    parse_array_type(type_)
        .map(|(_, elem)| elem)
        .or_else(|| parse_slice_type(type_))
}

fn is_sequence_type(type_: &str) -> bool {
    sequence_element_type(type_).is_some()
}

fn enum_payload_type_name(enum_name: &str, case_name: &str) -> String {
    format!("{enum_name}.{case_name}")
}

fn method_receiver_base_type(type_: &str) -> String {
    type_.strip_prefix('*').unwrap_or(type_).to_string()
}

fn method_full_name(receiver: &str, name: &str) -> String {
    format!("{}.{}", method_receiver_base_type(receiver), name)
}

fn function_signature_key(function: &FunctionDecl) -> String {
    if let Some(receiver) = &function.receiver {
        return method_full_name(&receiver.type_ref.to_string(), &function.name);
    }
    function.name.clone()
}

fn call_name(expr: &Expression) -> Option<(String, crate::token::Position)> {
    match expr {
        Expression::Ident(expr) => Some((expr.name.clone(), expr.name_pos.clone())),
        _ => None,
    }
}

fn direct_function_literal(expr: &Expression) -> Option<&FunctionLiteralExpr> {
    match expr {
        Expression::FunctionLiteral(literal) => Some(literal),
        Expression::Group(group) => direct_function_literal(&group.inner),
        _ => None,
    }
}

fn package_for_method_receiver(receiver: &str) -> String {
    package_for_type(&method_receiver_base_type(receiver))
}

fn package_for_function(full_name: &str) -> String {
    if full_name == "main" {
        return "main".to_string();
    }
    last_top_level_dot(full_name)
        .filter(|idx| *idx > 0)
        .map(|idx| full_name[..idx].to_string())
        .unwrap_or_default()
}

fn package_for_type(type_: &str) -> String {
    last_top_level_dot(type_)
        .filter(|idx| *idx > 0)
        .map(|idx| type_[..idx].to_string())
        .unwrap_or_default()
}

fn package_display_name(package: &str) -> &str {
    package.rsplit_once('.').map_or(package, |(_, name)| name)
}

fn last_top_level_dot(text: &str) -> Option<usize> {
    let mut depth = 0;
    for (idx, ch) in text.char_indices().rev() {
        match ch {
            ']' => depth += 1,
            '[' if depth > 0 => depth -= 1,
            '.' if depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
}

pub(crate) fn const_integer_expression(expr: &Expression) -> Option<i64> {
    match expr {
        Expression::Int(expr) => Some(expr.value),
        Expression::Char(expr) => Some(expr.value as i64),
        Expression::Group(expr) => const_integer_expression(&expr.inner),
        Expression::Unary(expr) if expr.operator == Kind::Minus => {
            const_integer_expression(&expr.inner)?.checked_neg()
        }
        Expression::Binary(expr) => {
            let left = const_integer_expression(&expr.left)?;
            let right = const_integer_expression(&expr.right)?;
            match expr.operator {
                Kind::Plus => left.checked_add(right),
                Kind::Minus => left.checked_sub(right),
                Kind::Star => left.checked_mul(right),
                Kind::Slash if right != 0 => left.checked_div(right),
                Kind::Percent if right != 0 => left.checked_rem(right),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn is_untyped_integer_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Int(_) => true,
        Expression::Group(expr) => is_untyped_integer_expression(&expr.inner),
        Expression::Unary(expr) if expr.operator == Kind::Minus => {
            is_untyped_integer_expression(&expr.inner)
        }
        Expression::Binary(expr)
            if matches!(
                expr.operator,
                Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash | Kind::Percent
            ) =>
        {
            is_untyped_integer_expression(&expr.left) && is_untyped_integer_expression(&expr.right)
        }
        _ => false,
    }
}

pub(crate) fn infer_untyped_integer_type(expr: &Expression) -> Option<&'static str> {
    if !is_untyped_integer_expression(expr) {
        return None;
    }
    if let Some(value) = const_integer_expression(expr) {
        return Some(if int_value_fits(value, TYPE_I32) {
            TYPE_I32
        } else {
            TYPE_I64
        });
    }
    if int_expression_operands_fit(expr, TYPE_I32) {
        Some(TYPE_I32)
    } else if int_expression_operands_fit(expr, TYPE_I64) {
        Some(TYPE_I64)
    } else {
        None
    }
}

fn int_expression_operands_fit(expr: &Expression, target: &str) -> bool {
    if let Some(value) = const_integer_expression(expr) {
        return int_value_fits(value, target);
    }
    match expr {
        Expression::Group(expr) => int_expression_operands_fit(&expr.inner, target),
        Expression::Unary(expr) if expr.operator == Kind::Minus => {
            int_expression_operands_fit(&expr.inner, target)
        }
        Expression::Binary(expr)
            if matches!(
                expr.operator,
                Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash | Kind::Percent
            ) =>
        {
            int_expression_operands_fit(&expr.left, target)
                && int_expression_operands_fit(&expr.right, target)
        }
        _ => false,
    }
}

fn int_value_fits(value: i64, target: &str) -> bool {
    match target {
        TYPE_I32 => i32::try_from(value).is_ok(),
        TYPE_I64 => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::{
        lower::lower_package_graph, mono::monomorphize_program, package::load_package_graph,
        parser::parse_file,
    };

    use super::*;

    #[test]
    fn keeps_process_limit_errors_separate_from_environment_errors() {
        let mut checker = Checker::new();
        checker.register_host_error_names(&test_host_signature("process.run"));
        for name in [
            "Cancelled",
            "IO",
            "InvalidArgument",
            "LimitExceeded",
            "NotFound",
            "PermissionDenied",
            "Timeout",
        ] {
            assert!(
                checker.info.error_codes.contains_key(name),
                "missing {name}"
            );
        }

        let mut checker = Checker::new();
        checker.register_host_error_names(&test_host_signature("env.lookup"));
        for name in ["IO", "InvalidArgument", "NotFound", "PermissionDenied"] {
            assert!(
                checker.info.error_codes.contains_key(name),
                "missing {name}"
            );
        }
        for name in ["Cancelled", "LimitExceeded", "Timeout"] {
            assert!(
                !checker.info.error_codes.contains_key(name),
                "environment lookup registered process-only error {name}"
            );
        }
    }

    fn test_host_signature(full_name: &str) -> Signature {
        Signature {
            name: full_name.to_string(),
            package: full_name.split_once('.').unwrap().0.to_string(),
            full_name: full_name.to_string(),
            method: false,
            receiver: String::new(),
            params: Vec::new(),
            return_type: TYPE_VOID.to_string(),
            errorable: true,
            builtin: false,
            host_intrinsic: true,
            exported: true,
        }
    }

    #[test]
    fn private_field_ownership_uses_canonical_package_identity() {
        let (mut program, diagnostics) = parse_file(
            "<test>",
            r#"package main

struct Secret {
    secret i32
}

fn inspect(value Secret) i32 {
    return value.secret
}
"#,
        );
        assert_eq!(diagnostics, Vec::new());

        let secret_type = "origin_a.model.Secret".to_string();
        program.structs[0].name = secret_type.clone();
        program.functions[0].name = "origin_b.model.inspect".to_string();
        program.functions[0].params[0].type_ref.name = secret_type;

        let (_info, diagnostics) = check_program(&program);

        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message
            == "field \"secret\" of struct \"origin_a.model.Secret\" is not exported by package \"model\""));
    }

    #[test]
    fn checks_declarations_for_every_testdata_entry_without_diagnostics() {
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
            let (mono, mono_diagnostics) = monomorphize_program(&lowered);
            if !mono_diagnostics.is_empty() {
                failures.push(format!("{} mono: {:?}", entry.display(), mono_diagnostics));
                continue;
            }
            let (_info, check_diagnostics) = check_program_metadata(&mono);
            if !check_diagnostics.is_empty() {
                failures.push(format!(
                    "{} check: {:?}",
                    entry.display(),
                    check_diagnostics
                ));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn builds_struct_enum_interface_and_function_metadata() {
        let program = parse_ok(
            r#"
package main

struct Point {
    x i32
    y i32
}

interface Writer {
    write(msg str) !void
}

enum Result {
    Ok { value i32 }
    Err { message str }
}

fn (p *Point) reset() void {
    return
}

fn main() i32 {
    return 0
}
"#,
        );

        let (info, diagnostics) = check_program(&program);
        assert_eq!(diagnostics, Vec::new());
        assert_eq!(info.structs["Point"].fields[0].type_, TYPE_I32);
        assert_eq!(info.interfaces["Writer"].methods[0].return_type, TYPE_VOID);
        assert_eq!(info.enums["Result"].cases[0].payload_type, "Result.Ok");
        assert!(info.methods["*Point"].contains_key("reset"));
        assert_eq!(info.functions["main"].return_type, TYPE_I32);
    }

    #[test]
    fn checks_function_bodies_for_every_testdata_entry_without_diagnostics() {
        let root = repo_root();
        let mut entries = Vec::new();
        collect_main_files(&root.join("testdata"), &mut entries);
        entries.sort();

        for path in entries {
            let fixture = path.strip_prefix(&root).unwrap_or(&path).display();
            let (graph, diagnostics) = load_package_graph(&path, false).unwrap();
            assert_eq!(diagnostics, Vec::new(), "{fixture} load");
            let (lowered, diagnostics) = lower_package_graph(&graph);
            assert_eq!(diagnostics, Vec::new(), "{fixture} lower");
            let (mono, diagnostics) = monomorphize_program(&lowered);
            assert_eq!(diagnostics, Vec::new(), "{fixture} mono");
            let (_info, diagnostics) = check_program(&mono);
            assert_eq!(diagnostics, Vec::new(), "{fixture} check");
        }
    }

    #[test]
    fn checks_test_package_bodies_for_supported_fixtures() {
        let root = repo_root();
        for fixture in ["testdata/testing_basic", "testdata/testing_fail"] {
            let path = root.join(fixture);
            let (graph, diagnostics) = load_package_graph(&path, true).unwrap();
            assert_eq!(diagnostics, Vec::new(), "{fixture} load");
            let (lowered, diagnostics) = lower_package_graph(&graph);
            assert_eq!(diagnostics, Vec::new(), "{fixture} lower");
            let (mono, diagnostics) = monomorphize_program(&lowered);
            assert_eq!(diagnostics, Vec::new(), "{fixture} mono");
            let (_info, diagnostics) = check_program(&mono);
            assert_eq!(diagnostics, Vec::new(), "{fixture} check");
        }
    }

    #[test]
    fn rejects_invalid_core_function_bodies() {
        let program = parse_ok(
            r#"
package main

struct Point {
    x i32
}

fn add(a i32, b i32) i32 {
    return a + b
}

fn main() i32 {
    x := true
    y := add(1, x)
    p := Point{x: "bad"}
    if 1 {
        return y
    }
    return p.nope
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(&diagnostics, "argument 2 to \"add\" must be i32, got bool");
        assert_has(&diagnostics, "cannot assign str to i32");
        assert_has(&diagnostics, "if condition must be bool");
        assert_has(&diagnostics, "struct \"Point\" has no field \"nope\"");
    }

    #[test]
    fn rejects_invalid_error_sugar_with_stable_diagnostics() {
        let non_errorable_operand = parse_ok(
            r#"
package main

fn main() i32 {
    x := 1?
    return x
}
"#,
        );

        let (_info, diagnostics) = check_program(&non_errorable_operand);
        assert_has(
            &diagnostics,
            "? requires an errorable expression or error value",
        );
        assert_not_has(
            &diagnostics,
            "cannot use ? in a function that cannot return an error",
        );

        let invalid_propagation_context = parse_ok(
            r#"
package main

fn divide(a i32, b i32) !i32 {
    return a / b
}

fn main() i32 {
    x := divide(10, 2)?
    return x
}
"#,
        );

        let (_info, diagnostics) = check_program(&invalid_propagation_context);
        assert_has(
            &diagnostics,
            "cannot use ? in a function that cannot return an error",
        );

        let errorable_builtin_argument = parse_ok(
            r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    text := to_str(maybe())
    return len(text)
}
"#,
        );

        let (_info, diagnostics) = check_program(&errorable_builtin_argument);
        assert_has(
            &diagnostics,
            "errorable value cannot be passed as an argument",
        );
    }

    #[test]
    fn rejects_errorable_special_builtin_arguments() {
        let cases = [
            (
                "append slice argument",
                "errorable value cannot be passed as an argument",
                r#"
package main

fn maybe_values() ![]i32 {
    return []i32{}
}

fn main() i32 {
    values := append(maybe_values(), 1)
    return len(values)
}
"#,
            ),
            (
                "append value argument",
                "errorable value cannot be passed as an argument",
                r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    values := []i32{}
    values = append(values, maybe())
    return len(values)
}
"#,
            ),
            (
                "append requires value",
                "argument 2 to \"append\" requires a value",
                r#"
package main

fn noop() void {
    return
}

fn main() i32 {
    values := []i32{}
    values = append(values, noop())
    return len(values)
}
"#,
            ),
            (
                "has map argument",
                "errorable value cannot be passed as an argument",
                r#"
package main

fn maybe_map() !map[i32]i32 {
    return map[i32]i32{}
}

fn main() i32 {
    if has(maybe_map(), 1) {
        return 1
    }
    return 0
}
"#,
            ),
            (
                "has key argument",
                "errorable value cannot be passed as an argument",
                r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    values := map[i32]i32{}
    if has(values, maybe()) {
        return 1
    }
    return 0
}
"#,
            ),
            (
                "delete key argument",
                "errorable value cannot be passed as an argument",
                r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    values := map[i32]i32{}
    delete(values, maybe())
    return 0
}
"#,
            ),
            (
                "keys map argument",
                "errorable value cannot be passed as an argument",
                r#"
package main

fn maybe_map() !map[i32]i32 {
    return map[i32]i32{}
}

fn main() i32 {
    values := keys(maybe_map())
    return len(values)
}
"#,
            ),
        ];

        for (name, expected, src) in cases {
            let program = parse_ok(src);
            let (_info, diagnostics) = check_program(&program);
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message == expected),
                "{name}: missing diagnostic {expected:?}; got {diagnostics:?}",
            );
        }
    }

    #[test]
    fn rejects_value_or_handlers_that_do_not_terminate_control_flow() {
        let program = parse_ok(
            r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    value := maybe() or |err| {
        print(to_str(err))
    }
    return value
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "or handler for a value result must terminate control flow",
        );
    }

    #[test]
    fn accepts_value_or_handlers_terminated_by_exhaustive_match() {
        let program = parse_ok(
            r#"
package main

enum Mode {
    Zero
    One
}

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    mode := Mode.Zero
    value := maybe() or |_| {
        match mode {
        case Mode.Zero {
            return 0
        }
        case Mode.One {
            return 1
        }
        }
    }
    return value
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn rejects_invalid_slice_bounds_with_stable_diagnostics() {
        let program = parse_ok(
            r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    values := []i32{1, 2, 3}
    errorable := values[maybe():2]
    non_integer := values[true:2]
    too_large := values[2147483648:2]
    return len(errorable) + len(non_integer) + len(too_large)
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(&diagnostics, "slice bounds cannot be errorable");
        assert_has(&diagnostics, "slice bounds must be integers");
        assert_has(&diagnostics, "slice bounds must fit in i32");
    }

    #[test]
    fn rejects_invalid_map_assignment_targets_with_stable_diagnostics() {
        let cases = [
            (
                "errorable map target",
                "indexing cannot use an errorable value",
                r#"
package main

fn maybe_map() !map[i32]i32 {
    return map[i32]i32{}
}

fn main() i32 {
    maybe_map()[1] = 2
    return 0
}
"#,
            ),
            (
                "errorable map key",
                "map key expression cannot be errorable",
                r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    values := map[i32]i32{}
    values[maybe()] = 2
    return 0
}
"#,
            ),
        ];

        for (name, expected, src) in cases {
            let program = parse_ok(src);
            let (_info, diagnostics) = check_program(&program);
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message == expected),
                "{name}: missing diagnostic {expected:?}; got {diagnostics:?}",
            );
        }
    }

    #[test]
    fn rejects_map_compound_assignment_with_stable_diagnostic() {
        let program = parse_ok(
            r#"
package main

fn main() i32 {
    values := map[str]i32{"count": 1}
    values["count"] += 1
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "map compound assignment is not supported; use an explicit lookup and assignment",
        );
    }

    #[test]
    fn suppresses_follow_up_diagnostics_after_invalid_expression_types() {
        let program = parse_ok(
            r#"
package main

fn main() i32 {
    x := missing()
    var y i32 = missing()
    y = missing()
    return missing()
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);

        assert_has(&diagnostics, "unknown function \"missing\"");
        assert_not_has(&diagnostics, "declaration requires a value");
        assert_not_has(&diagnostics, "cannot assign  to i32");
        assert_not_has(&diagnostics, "cannot return  from function returning i32");
    }

    #[test]
    fn rejects_errorable_conditions_with_stable_diagnostics() {
        let program = parse_ok(
            r#"
package main

fn maybe() !bool {
    return true
}

fn main() i32 {
    if maybe() {
        return 1
    }
    for maybe() {
        return 2
    }
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(&diagnostics, "if condition cannot be errorable");
        assert_has(&diagnostics, "for condition cannot be errorable");
        assert_not_has(
            &diagnostics,
            "if condition cannot be errorable; must be bool",
        );
        assert_not_has(
            &diagnostics,
            "for condition cannot be errorable; must be bool",
        );
    }

    #[test]
    fn checks_loop_control_and_noreturn_flow() {
        let valid = parse_ok(
            r#"
package main

fn stop() noreturn {
    panic("stop")
}

fn main() i32 {
    for {
        break
    }
    for i := 0; i < 2; i = i + 1 {
        continue
    }
    stop()
}
"#,
        );

        let (_info, diagnostics) = check_program(&valid);
        assert_eq!(diagnostics, Vec::new());

        let invalid = parse_ok(
            r#"
package main

fn main() i32 {
    break
    continue
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&invalid);
        assert_has(&diagnostics, "break can only be used inside a loop");
        assert_has(&diagnostics, "continue can only be used inside a loop");
    }

    #[test]
    fn checks_closures_and_rejects_invalid_captures() {
        let valid = parse_ok(
            r#"
package main

fn apply_twice(f fn(i32) i32, value i32) i32 {
    return f(f(value))
}

fn make_adder(base i32) fn(i32) i32 {
    return fn(delta i32) i32 {
        return base + delta
    }
}

fn main() i32 {
    base := 10
    add := make_adder(base)
    inc := fn(value i32) i32 {
        return value + 1
    }
    return apply_twice(add, inc(1))
}
"#,
        );

        let (_info, diagnostics) = check_program(&valid);
        assert_eq!(diagnostics, Vec::new());

        let invalid = parse_ok(
            r#"
package main

fn main() i32 {
    value := 1
    assign := fn() i32 {
        value = 2
        return value
    }
    address := fn() i32 {
        ptr := &value
        if ptr == nil {
            return 1
        }
        return value
    }
    call := fn(value i32) i32 {
        return value
    }
    return assign() + address() + call()
}
"#,
        );

        let (_info, diagnostics) = check_program(&invalid);
        assert_has(
            &diagnostics,
            "cannot assign to captured outer local \"value\"",
        );
        assert_has(
            &diagnostics,
            "captured outer local \"value\" is not addressable",
        );
        assert_not_has(
            &diagnostics,
            "address-of requires an addressable operand or composite literal",
        );
        assert_has(
            &diagnostics,
            "function \"function value\" expects 1 arguments, got 0",
        );
    }

    #[test]
    fn checks_concurrency_and_rejects_invalid_spawn_channel_use() {
        let valid = parse_ok(
            r#"
package main

fn compute(v i32) i32 {
    return v * 2
}

fn read_one(ch chan[i32]) !i32 {
    return chan_recv(ch)
}

fn main() i32 {
    ch := chan_new[i32](2)
    chan_send(ch, 3) or |_| {
        return 1
    }

    values := taskgroup []i32 {
        spawn compute(1)
        spawn compute(2)
    }

    results := taskgroup []!i32 {
        spawn read_one(ch)
    }

    first := results[0] or |_| {
        return 2
    }
    if first != 3 {
        return 3
    }
    if len(values) != 2 {
        return 4
    }
    chan_close(ch)
    return values[0] + values[1]
}
"#,
        );

        let (_info, diagnostics) = check_program(&valid);
        assert_eq!(diagnostics, Vec::new());

        let invalid = parse_ok(
            r#"
package main

fn work() i32 {
    return 1
}

fn main() i32 {
    spawn work()

    f := fn() i32 {
        return 1
    }
    bad_call := taskgroup []i32 {
        spawn f
    }

    bad_return := taskgroup []str {
        spawn work()
    }

    values := []i32{1, 2}
    bad_builtin := taskgroup []i32 {
        spawn len(values)
    }

    ch := chan_new[i32](1)
    chan_send(ch, "x")
    nested := chan_new[chan[i32]](1)

    for true {
        taskgroup []void {
            spawn fn() void {
                return
            }()
            break
        }
    }
    return len(bad_call) + len(bad_return)
}
"#,
        );

        let (_info, diagnostics) = check_program(&invalid);
        assert_has(&diagnostics, "spawn is only valid inside a taskgroup body");
        assert_has(&diagnostics, "spawn requires a call expression");
        assert_has(&diagnostics, "spawned call must return str, got i32");
        assert_has(
            &diagnostics,
            "argument 2 to \"chan_send\" must be i32, got str",
        );
        assert_has(
            &diagnostics,
            "channel element type \"chan[i32]\" is not allowed",
        );
        assert_has(
            &diagnostics,
            "spawn does not currently support builtin \"len\" directly; wrap it in an inline function literal",
        );
        assert_has(&diagnostics, "break cannot exit a taskgroup body");
    }

    #[test]
    fn rejects_concurrency_constructs_that_cannot_reach_join_or_nest_errors() {
        let program = parse_ok(
            r#"
package main

fn stop() noreturn {
    panic("stop")
}

fn main() i32 {
    taskgroup []void {
        stop()
    }

    direct := chan_new[!i32](1)
    var declared chan[!str]
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "noreturn expression cannot be used inside a taskgroup body because it would skip the taskgroup join",
        );
        assert_has(&diagnostics, "channel element type \"!i32\" is not allowed");
        assert_has(&diagnostics, "channel element type \"!str\" is not allowed");
    }

    #[test]
    fn rejects_non_share_safe_spawn_arguments_transitively() {
        let program = parse_ok(
            r#"
package main

interface Reader {
    read() i32
}

struct ReaderImpl {
    value i32
}

fn (reader ReaderImpl) read() i32 {
    return reader.value
}

struct WithSlice {
    items []i32
}

enum Choice {
    Safe { value i32 }
    Unsafe { values map[str]i32 }
}

struct Box[T] {
    value T
}

fn take_pointer(value *i32) void { return }
fn take_slice(value []i32) void { return }
fn take_map(value map[str]i32) void { return }
fn take_interface(value Reader) void { return }
fn take_function(value fn() void) void { return }
fn take_array(value [1][]i32) void { return }
fn take_struct(value WithSlice) void { return }
fn take_enum(value Choice) void { return }
fn take_box(value Box[map[str]i32]) void { return }
fn take_channel(value chan[WithSlice]) void { return }

fn main() i32 {
    number := 1
    items := []i32{1}
    lookup := map[str]i32{"a": 1}
    callback := fn() void { return }
    reader := ReaderImpl{value: 1}
    channel := chan_new[WithSlice](1)

    taskgroup []void {
        spawn take_pointer(&number)
        spawn take_slice(items)
        spawn take_map(lookup)
        spawn take_interface(reader)
        spawn take_function(callback)
        spawn take_array([1][]i32{items})
        spawn take_struct(WithSlice{items: items})
        spawn take_enum(Choice.Safe{value: 1})
        spawn take_box(Box[map[str]i32]{value: lookup})
        spawn take_channel(channel)
    }

    chan_close(channel)
    return 0
}
"#,
        );
        let (program, diagnostics) = monomorphize_program(&program);
        assert_eq!(diagnostics, Vec::new());

        let (_info, diagnostics) = check_program(&program);

        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.message.contains("cannot cross a task boundary"))
                .count(),
            10,
            "unexpected diagnostics: {diagnostics:?}",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: type \"*i32\" is not share-safe",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: type \"[]i32\" is not share-safe",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: type \"map[str]i32\" is not share-safe",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: type \"Reader\" is not share-safe",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: type \"fn() void\" is not share-safe",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: array element has non-share-safe type \"[]i32\"",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: field \"items\" has non-share-safe type \"[]i32\"",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: enum case \"Unsafe\" -> field \"values\" has non-share-safe type \"map[str]i32\"",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: field \"value\" has non-share-safe type \"map[str]i32\"",
        );
        assert_has(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: channel element -> field \"items\" has non-share-safe type \"[]i32\"",
        );
    }

    #[test]
    fn keeps_spawn_capture_types_separate_across_generic_instantiations() {
        let program = parse_ok(
            r#"
package main

fn launch[T](value T) i32 {
    results := taskgroup []i32 {
        spawn fn() i32 {
            value
            return 1
        }()
    }
    return results[0]
}

fn main() i32 {
    safe := launch[i32](1)
    unsafe := launch[[]i32]([]i32{1})
    return safe + unsafe
}
"#,
        );
        let (program, diagnostics) = monomorphize_program(&program);
        assert_eq!(diagnostics, Vec::new());

        let (info, diagnostics) = check_program(&program);

        assert_has(
            &diagnostics,
            "spawned closure capture \"value\" cannot cross a task boundary: type \"[]i32\" is not share-safe",
        );
        let mut capture_types = info
            .function_literals
            .values()
            .flat_map(|literal| literal.captures.iter())
            .filter(|capture| capture.name == "value")
            .map(|capture| capture.type_.as_str())
            .collect::<Vec<_>>();
        capture_types.sort_unstable();
        assert_eq!(capture_types, vec!["[]i32", "i32"]);
    }

    #[test]
    fn rejects_unsafe_captures_and_unsupported_spawn_targets() {
        let program = parse_ok(
            r#"
package main

struct Worker {
    value i32
}

fn (worker Worker) run() i32 {
    return worker.value
}

fn main() i32 {
    items := []i32{1}
    callable := fn(value i32) i32 {
        return value
    }
    callback := fn() i32 {
        return 1
    }
    worker := Worker{value: 1}
    channel := chan_new[i32](1)

    captured := taskgroup []i32 {
        spawn fn() i32 {
            return items[0]
        }()
    }

    nested_capture := taskgroup []i32 {
        spawn fn() i32 {
            read := fn() i32 {
                return items[0]
            }
            return read()
        }()
    }

    function_capture := taskgroup []i32 {
        spawn fn() i32 {
            return callback()
        }()
    }

    opaque := taskgroup []i32 {
        spawn callable(1)
    }

    method := taskgroup []i32 {
        spawn worker.run()
    }

    taskgroup []void {
        spawn chan_close(channel)
    }

    return len(captured) + len(nested_capture) + len(function_capture) + len(opaque) + len(method)
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);

        assert_has(
            &diagnostics,
            "spawned closure capture \"items\" cannot cross a task boundary: type \"[]i32\" is not share-safe",
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic
                    .message
                    .starts_with("spawned closure capture \"items\" cannot cross a task boundary"))
                .count(),
            2,
            "unexpected diagnostics: {diagnostics:?}",
        );
        assert_has(
            &diagnostics,
            "spawn cannot call an arbitrary function value; use a named function or an inline function literal with share-safe captures",
        );
        assert_has(
            &diagnostics,
            "spawned closure capture \"callback\" cannot cross a task boundary: type \"fn() i32\" is not share-safe",
        );
        assert_has(
            &diagnostics,
            "spawn does not currently support selector or method calls directly; wrap the call in an inline function literal",
        );
        assert_has(
            &diagnostics,
            "spawn does not currently support builtin \"chan_close\" directly; wrap it in an inline function literal",
        );
    }

    #[test]
    fn rejects_propagation_that_can_exit_a_taskgroup_body() {
        let valid = parse_ok(
            r#"
package main

fn maybe() !i32 {
    return 1
}

fn wrapped() !i32 {
    values := taskgroup []!i32 {
        spawn fn() !i32 {
            return maybe()?
        }()
    }
    return values[0]?
}

fn main() i32 {
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&valid);
        assert_eq!(diagnostics, Vec::new());

        let invalid = parse_ok(
            r#"
package main

fn maybe() !i32 {
    return 1
}

fn consume(value i32) i32 {
    return value
}

fn invalid() !i32 {
    values := taskgroup []i32 {
        spawn consume(maybe()?)
        direct := maybe()?
        if true {
            nested := maybe()?
        }
        for false {
            inside_loop := maybe()?
        }
        error.Fail or |_| {
            inside_handler := maybe()?
        }
        inner := taskgroup []i32 {
            inside_nested_group := maybe()?
        }
        fallible := fn() !i32 {
            return maybe()?
        }
        from_closure := fallible()?
        error.Fail?
    }
    return len(values)
}

fn main() i32 {
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&invalid);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.message
                        == "? is not allowed inside a taskgroup body because it could skip the taskgroup join"
                })
                .count(),
            8,
            "{diagnostics:?}"
        );
    }

    #[test]
    fn accepts_channel_equality_comparisons() {
        let program = parse_ok(
            r#"
package main

fn main() i32 {
    ch := chan_new[i32](1)
    same := ch == ch
    different := ch != ch
    if !same || different {
        return 1
    }
    chan_close(ch)
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_eq!(diagnostics, Vec::new());
    }

    #[test]
    fn rejects_invalid_method_and_interface_bodies() {
        let program = parse_ok(
            r#"
package main

interface Labeler {
    label() str
}

struct User {
    name str
}

fn (u User) label(prefix str) str {
    return prefix + u.name
}

fn want(v Labeler) str {
    return v.missing()
}

fn main() i32 {
    u := User{name: "ada"}
    print(u.rename())
    print(want(u))
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "interface \"Labeler\" has no method \"missing\"",
        );
        assert_has(&diagnostics, "type \"User\" has no method \"rename\"");
        assert_has(
            &diagnostics,
            "method \"label\" on \"User\" does not match interface \"Labeler\"",
        );
    }

    #[test]
    fn rejects_interface_argument_mismatch_with_stable_diagnostic() {
        let program = parse_ok(
            r#"
package main

interface Writer {
    write(s str) void
}

interface Reader {
    read() str
}

fn consume(w Writer) void {
    return
}

fn main() i32 {
    var r Reader
    consume(r)
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "argument 1 to \"consume\" must be Writer, got Reader",
        );
        assert_not_has(&diagnostics, "cannot assign Reader to Writer");
        assert_not_has(
            &diagnostics,
            "argument 1 to \"consume\" must be Writer, got ",
        );
    }

    #[test]
    fn rejects_method_values() {
        let program = parse_ok(
            r#"
package main

interface Labeler {
    label() str
}

struct User {
    name str
}

fn (u User) label() str {
    return u.name
}

fn use_interface(value Labeler) void {
    method := value.label
    return
}

fn main() i32 {
    user := User{name: "ada"}
    method := user.label
    use_interface(user)
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(&diagnostics, "method values are not supported");
    }

    #[test]
    fn rejects_invalid_match_bodies() {
        let program = parse_ok(
            r#"
package main

enum TokenKind {
    Ident
    Int
}

enum Other {
    Text
}

enum Value {
    Number { value i32 }
}

fn main() i32 {
    kind := TokenKind.Ident
    bad := Value.Number("bad")
    match kind {
    case TokenKind.Ident(_) {
        return 1
    }
    case TokenKind.Missing {
        return 2
    }
    case Other.Text {
        return 3
    }
    case TokenKind.Ident {
        return 4
    }
    }

    match 1 {
    else {
        return 5
    }
    }
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(&diagnostics, "cannot assign str to i32");
        assert_has(
            &diagnostics,
            "plain enum case \"Ident\" cannot bind a payload",
        );
        assert_has(&diagnostics, "enum \"TokenKind\" has no case \"Missing\"");
        assert_has(&diagnostics, "match arm must use enum \"TokenKind\"");
        assert_has(&diagnostics, "duplicate match arm for \"Ident\"");
        assert_has(
            &diagnostics,
            "match on \"TokenKind\" is not exhaustive; missing TokenKind.Int",
        );
        assert_has(&diagnostics, "match requires an enum value");
    }

    #[test]
    fn rejects_enum_equality_with_stable_diagnostic() {
        let program = parse_ok(
            r#"
package main

enum TokenKind {
    Ident
    Int
}

fn main() i32 {
    ok := TokenKind.Ident == TokenKind.Int
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "comparison is not supported for enum values in v0.4",
        );
    }

    #[test]
    fn rejects_unsupported_equality_with_stable_diagnostics() {
        let program = parse_ok(
            r#"
package main

struct Point {
    x i32
}

fn main() i32 {
    same := Point{x: 1} == Point{x: 2}
    mixed := Point{x: 1} == true
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "comparison is only supported for bool, integers, pointers, str, and error",
        );
        assert_has(&diagnostics, "comparison operands must have the same type");
    }

    #[test]
    fn rejects_bare_nil_equality_with_stable_diagnostic() {
        let program = parse_ok(
            r#"
package main

fn main() i32 {
    same := nil == nil
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "comparison is only supported for bool, integers, pointers, str, and error",
        );
    }

    #[test]
    fn rejects_invalid_declaration_shapes() {
        let program = parse_ok(
            r#"
package main

struct Bad {
    value void
    value i32
}

struct Loop {
    next Loop
}

fn print() void {
    return
}

fn len(value i32) i32 {
    return value
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(&diagnostics, "field \"value\" cannot use type \"void\"");
        assert_has(
            &diagnostics,
            "field \"value\" is already declared in struct \"Bad\"",
        );
        assert_has(
            &diagnostics,
            "struct \"Loop\" cannot contain itself recursively",
        );
        assert_has(&diagnostics, "function \"print\" is already declared");
        assert_has(&diagnostics, "function \"len\" is already declared");
        assert_has(&diagnostics, "missing main function");
    }

    #[test]
    fn rejects_invalid_type_refs_and_method_receivers() {
        let program = parse_ok(
            r#"
package main

struct Box {
    items map[[]i32]i32
}

fn (x i32) bad() void {
    return
}

fn main() i32 {
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "map key type \"[]i32\" is not supported; must be bool, i32, i64, or str",
        );
        assert_has(
            &diagnostics,
            "method receiver must be a named struct type or pointer to one",
        );
    }

    #[test]
    fn rejects_unmonomorphized_generic_declarations() {
        let program = parse_ok(
            r#"
package main

struct Box[T] {
    value T
}

fn id[T](value T) T {
    return value
}

fn main() i32 {
    return 0
}
"#,
        );

        let (_info, diagnostics) = check_program(&program);
        assert_has(
            &diagnostics,
            "generic struct \"Box\" must be monomorphized before checking",
        );
        assert_has(
            &diagnostics,
            "generic function \"id\" must be monomorphized before checking",
        );
    }

    fn parse_ok(src: &str) -> Program {
        let (program, diagnostics) = parse_file("<test>", src);
        assert_eq!(diagnostics, Vec::new());
        program
    }

    fn assert_has(diagnostics: &[Diagnostic], expected: &str) {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message == expected),
            "missing diagnostic {expected:?}; got {diagnostics:?}",
        );
    }

    fn assert_not_has(diagnostics: &[Diagnostic], unexpected: &str) {
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.message != unexpected),
            "unexpected diagnostic {unexpected:?}; got {diagnostics:?}",
        );
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
