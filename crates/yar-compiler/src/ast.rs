use std::{collections::BTreeMap, fmt};

use crate::token::{Kind, Position};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    pub package_pos: Position,
    pub package_name: String,
    pub imports: Vec<ImportDecl>,
    pub structs: Vec<StructDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub enums: Vec<EnumDecl>,
    pub functions: Vec<FunctionDecl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageImport {
    pub name: String,
    pub path: String,
    pub target: PackageId,
    pub decl: ImportDecl,
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum SourceId {
    #[default]
    Entry,
    Path {
        manifest_path: String,
    },
    Git {
        git: String,
        commit: String,
    },
    Stdlib,
}

/// Stable package identity inside one compilation graph.
///
/// Import spellings and dependency aliases are bindings owned by a source;
/// they are deliberately not part of the package identity.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct PackageId {
    pub source: SourceId,
    pub subpath: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Package {
    pub id: PackageId,
    pub name: String,
    pub stdlib: bool,
    pub files: Vec<Program>,
    pub imports: Vec<PackageImport>,
    pub structs: Vec<StructDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub enums: Vec<EnumDecl>,
    pub functions: Vec<FunctionDecl>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackageGraph {
    pub entry: PackageId,
    pub packages: BTreeMap<PackageId, Package>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeRefKind {
    Named,
    Errorable,
    Pointer,
    Array,
    Slice,
    Map,
    Chan,
    Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeRef {
    pub kind: TypeRefKind,
    pub name: String,
    pub type_args: Vec<TypeRef>,
    pub array_len: i64,
    pub elem: Option<Box<TypeRef>>,
    pub key: Option<Box<TypeRef>>,
    pub value: Option<Box<TypeRef>>,
    pub params: Vec<TypeRef>,
    pub return_type: Option<Box<TypeRef>>,
    pub errorable: bool,
    pub pos: Position,
}

impl TypeRef {
    pub fn named(name: impl Into<String>, pos: Position) -> Self {
        Self {
            kind: TypeRefKind::Named,
            name: name.into(),
            type_args: Vec::new(),
            array_len: 0,
            elem: None,
            key: None,
            value: None,
            params: Vec::new(),
            return_type: None,
            errorable: false,
            pos,
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TypeRefKind::Errorable => write_optional_prefix(f, "!", self.elem.as_deref()),
            TypeRefKind::Pointer => write_optional_prefix(f, "*", self.elem.as_deref()),
            TypeRefKind::Array => {
                write!(f, "[{}]", self.array_len)?;
                if let Some(elem) = self.elem.as_deref() {
                    write!(f, "{elem}")?;
                }
                Ok(())
            }
            TypeRefKind::Slice => write_optional_prefix(f, "[]", self.elem.as_deref()),
            TypeRefKind::Map => {
                write!(f, "map[")?;
                if let Some(key) = self.key.as_deref() {
                    write!(f, "{key}")?;
                }
                write!(f, "]")?;
                if let Some(value) = self.value.as_deref() {
                    write!(f, "{value}")?;
                }
                Ok(())
            }
            TypeRefKind::Chan => {
                write!(f, "chan[")?;
                if let Some(elem) = self.elem.as_deref() {
                    write!(f, "{elem}")?;
                }
                write!(f, "]")
            }
            TypeRefKind::Function => {
                let params = self
                    .params
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "fn({params}) ")?;
                if self.errorable {
                    write!(f, "!")?;
                }
                if let Some(return_type) = self.return_type.as_deref() {
                    write!(f, "{return_type}")?;
                }
                Ok(())
            }
            TypeRefKind::Named => {
                if self.type_args.is_empty() {
                    return f.write_str(&self.name);
                }
                let args = self
                    .type_args
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{}[{args}]", self.name)
            }
        }
    }
}

fn write_optional_prefix(
    f: &mut fmt::Formatter<'_>,
    prefix: &str,
    elem: Option<&TypeRef>,
) -> fmt::Result {
    f.write_str(prefix)?;
    if let Some(elem) = elem {
        write!(f, "{elem}")?;
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportDecl {
    pub import_pos: Position,
    pub path: String,
    pub path_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeParam {
    pub name: String,
    pub pos: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructDecl {
    pub struct_pos: Position,
    pub exported: bool,
    pub resource: bool,
    pub name: String,
    pub name_pos: Position,
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<StructField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructField {
    pub name: String,
    pub name_pos: Position,
    pub type_ref: TypeRef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceDecl {
    pub interface_pos: Position,
    pub exported: bool,
    pub name: String,
    pub name_pos: Position,
    pub methods: Vec<InterfaceMethodDecl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceMethodDecl {
    pub name: String,
    pub name_pos: Position,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub return_is_bang: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub enum_pos: Position,
    pub exported: bool,
    pub name: String,
    pub name_pos: Position,
    pub cases: Vec<EnumCaseDecl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumCaseDecl {
    pub name: String,
    pub name_pos: Position,
    pub fields: Vec<StructField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDecl {
    pub exported: bool,
    pub host_intrinsic: bool,
    pub name: String,
    pub name_pos: Position,
    pub type_params: Vec<TypeParam>,
    pub receiver: Option<ReceiverDecl>,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub return_is_bang: bool,
    pub body: BlockStmt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub name_pos: Position,
    pub type_ref: TypeRef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReceiverDecl {
    pub name: String,
    pub name_pos: Position,
    pub type_ref: TypeRef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockStmt {
    pub lbrace: Position,
    pub stmts: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Block(Box<BlockStmt>),
    Let(Box<LetStmt>),
    Var(Box<VarStmt>),
    Assign(Box<AssignStmt>),
    CompoundAssign(Box<CompoundAssignStmt>),
    If(Box<IfStmt>),
    For(Box<ForStmt>),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Return(Box<ReturnStmt>),
    Match(Box<MatchStmt>),
    Expr(Box<ExprStmt>),
    Spawn(Box<SpawnStmt>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LetStmt {
    pub let_pos: Position,
    pub name: String,
    pub name_pos: Position,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarStmt {
    pub var_pos: Position,
    pub name: String,
    pub name_pos: Position,
    pub type_ref: TypeRef,
    pub value: Option<Expression>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssignStmt {
    pub target: Expression,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompoundAssignStmt {
    pub target: Expression,
    pub operator: Kind,
    pub op_pos: Position,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfStmt {
    pub if_pos: Position,
    pub cond: Expression,
    pub then_block: BlockStmt,
    pub else_stmt: Option<Statement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForStmt {
    pub for_pos: Position,
    pub init: Option<Statement>,
    pub cond: Option<Expression>,
    pub post: Option<Statement>,
    pub body: BlockStmt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreakStmt {
    pub break_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinueStmt {
    pub continue_pos: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReturnStmt {
    pub return_pos: Position,
    pub value: Option<Expression>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchStmt {
    pub match_pos: Position,
    pub value: Expression,
    pub arms: Vec<MatchArm>,
    pub else_body: Option<BlockStmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchArm {
    pub case_pos: Position,
    pub enum_type: TypeRef,
    pub case_name: String,
    pub case_name_pos: Position,
    pub bind_name: String,
    pub bind_name_pos: Position,
    pub bind_ignore: bool,
    pub body: BlockStmt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExprStmt {
    pub expr: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpawnStmt {
    pub spawn_pos: Position,
    pub call: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Ident(Box<IdentExpr>),
    Int(IntLiteral),
    Char(CharLiteral),
    String(StringLiteral),
    Bool(BoolLiteral),
    Nil(NilLiteral),
    Error(ErrorLiteral),
    TypeApplication(Box<TypeApplicationExpr>),
    Call(Box<CallExpr>),
    FunctionLiteral(Box<FunctionLiteralExpr>),
    Taskgroup(Box<TaskgroupExpr>),
    Unary(Box<UnaryExpr>),
    Binary(Box<BinaryExpr>),
    Group(Box<GroupExpr>),
    Selector(Box<SelectorExpr>),
    Index(Box<IndexExpr>),
    Slice(Box<SliceExpr>),
    StructLiteral(Box<StructLiteralExpr>),
    ArrayLiteral(Box<ArrayLiteralExpr>),
    SliceLiteral(Box<SliceLiteralExpr>),
    MapLiteral(Box<MapLiteralExpr>),
    Propagate(Box<PropagateExpr>),
    Handle(Box<HandleExpr>),
    Missing(Position),
}

impl Expression {
    pub fn pos(&self) -> Position {
        match self {
            Expression::Ident(expr) => expr.name_pos.clone(),
            Expression::Int(expr) => expr.lit_pos.clone(),
            Expression::Char(expr) => expr.lit_pos.clone(),
            Expression::String(expr) => expr.lit_pos.clone(),
            Expression::Bool(expr) => expr.lit_pos.clone(),
            Expression::Nil(expr) => expr.lit_pos.clone(),
            Expression::Error(expr) => expr.err_pos.clone(),
            Expression::TypeApplication(expr) => expr.inner.pos(),
            Expression::Call(expr) => expr.callee.pos(),
            Expression::FunctionLiteral(expr) => expr.fn_pos.clone(),
            Expression::Taskgroup(expr) => expr.taskgroup_pos.clone(),
            Expression::Unary(expr) => expr.op_pos.clone(),
            Expression::Binary(expr) => expr.left.pos(),
            Expression::Group(expr) => expr.inner.pos(),
            Expression::Selector(expr) => expr.inner.pos(),
            Expression::Index(expr) => expr.inner.pos(),
            Expression::Slice(expr) => expr.inner.pos(),
            Expression::StructLiteral(expr) => expr.type_ref.pos.clone(),
            Expression::ArrayLiteral(expr) => expr.type_ref.pos.clone(),
            Expression::SliceLiteral(expr) => expr.type_ref.pos.clone(),
            Expression::MapLiteral(expr) => expr.type_ref.pos.clone(),
            Expression::Propagate(expr) => expr.inner.pos(),
            Expression::Handle(expr) => expr.inner.pos(),
            Expression::Missing(pos) => pos.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentExpr {
    pub name: String,
    pub name_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntLiteral {
    pub value: i64,
    pub lit_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharLiteral {
    pub value: char,
    pub lit_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringLiteral {
    pub value: String,
    pub lit_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoolLiteral {
    pub value: bool,
    pub lit_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NilLiteral {
    pub lit_pos: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorLiteral {
    pub name: String,
    pub err_pos: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeApplicationExpr {
    pub inner: Expression,
    pub lbracket_pos: Position,
    pub type_args: Vec<TypeRef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallExpr {
    pub callee: Expression,
    pub args: Vec<Expression>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionLiteralExpr {
    pub fn_pos: Position,
    pub enclosing_function: String,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub return_is_bang: bool,
    pub body: BlockStmt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskgroupExpr {
    pub taskgroup_pos: Position,
    pub result_type: TypeRef,
    pub body: BlockStmt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnaryExpr {
    pub operator: Kind,
    pub op_pos: Position,
    pub inner: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BinaryExpr {
    pub left: Expression,
    pub operator: Kind,
    pub op_pos: Position,
    pub right: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupExpr {
    pub inner: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectorExpr {
    pub inner: Expression,
    pub dot_pos: Position,
    pub name: String,
    pub name_pos: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexExpr {
    pub inner: Expression,
    pub lbracket_pos: Position,
    pub index: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SliceExpr {
    pub inner: Expression,
    pub lbracket_pos: Position,
    pub start: Option<Expression>,
    pub colon_pos: Position,
    pub end: Option<Expression>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructLiteralExpr {
    pub type_ref: TypeRef,
    pub lbrace: Position,
    pub fields: Vec<StructLiteralField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructLiteralField {
    pub name: String,
    pub name_pos: Position,
    pub value: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayLiteralExpr {
    pub type_ref: TypeRef,
    pub lbrace: Position,
    pub elements: Vec<Expression>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SliceLiteralExpr {
    pub type_ref: TypeRef,
    pub lbrace: Position,
    pub elements: Vec<Expression>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapLiteralExpr {
    pub type_ref: TypeRef,
    pub lbrace: Position,
    pub pairs: Vec<MapLiteralPair>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapLiteralPair {
    pub key: Expression,
    pub key_pos: Position,
    pub value: Expression,
    pub value_pos: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropagateExpr {
    pub inner: Expression,
    pub question_pos: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HandleExpr {
    pub inner: Expression,
    pub or_pos: Position,
    pub err_name: String,
    pub err_pos: Position,
    pub handler: BlockStmt,
}
