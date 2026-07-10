use std::collections::BTreeSet;

use crate::{ast::*, token::Kind};

pub(crate) fn collect_address_taken_locals(block: &BlockStmt) -> BTreeSet<String> {
    let mut locals = BTreeSet::new();
    collect_from_block(block, &mut locals);
    locals
}

fn collect_from_block(block: &BlockStmt, locals: &mut BTreeSet<String>) {
    for statement in &block.stmts {
        collect_from_statement(statement, locals);
    }
}

fn collect_from_statement(statement: &Statement, locals: &mut BTreeSet<String>) {
    match statement {
        Statement::Block(block) => collect_from_block(block, locals),
        Statement::Let(statement) => collect_from_expression(&statement.value, locals),
        Statement::Var(statement) => {
            if let Some(value) = &statement.value {
                collect_from_expression(value, locals);
            }
        }
        Statement::Assign(statement) => {
            collect_from_expression(&statement.target, locals);
            collect_from_expression(&statement.value, locals);
        }
        Statement::If(statement) => {
            collect_from_expression(&statement.cond, locals);
            collect_from_block(&statement.then_block, locals);
            if let Some(else_statement) = &statement.else_stmt {
                collect_from_statement(else_statement, locals);
            }
        }
        Statement::For(statement) => {
            if let Some(init) = &statement.init {
                collect_from_statement(init, locals);
            }
            if let Some(cond) = &statement.cond {
                collect_from_expression(cond, locals);
            }
            if let Some(post) = &statement.post {
                collect_from_statement(post, locals);
            }
            collect_from_block(&statement.body, locals);
        }
        Statement::Return(statement) => {
            if let Some(value) = &statement.value {
                collect_from_expression(value, locals);
            }
        }
        Statement::Match(statement) => {
            collect_from_expression(&statement.value, locals);
            for arm in &statement.arms {
                collect_from_block(&arm.body, locals);
            }
            if let Some(else_body) = &statement.else_body {
                collect_from_block(else_body, locals);
            }
        }
        Statement::Expr(statement) => collect_from_expression(&statement.expr, locals),
        Statement::Spawn(statement) => collect_from_expression(&statement.call, locals),
        Statement::Break(_) | Statement::Continue(_) => {}
    }
}

fn collect_from_expression(expression: &Expression, locals: &mut BTreeSet<String>) {
    match expression {
        Expression::TypeApplication(expression) => {
            collect_from_expression(&expression.inner, locals);
        }
        Expression::Call(expression) => {
            collect_from_expression(&expression.callee, locals);
            for argument in &expression.args {
                collect_from_expression(argument, locals);
            }
        }
        // Each function literal receives its own emitter and analysis pass.
        Expression::FunctionLiteral(_) => {}
        Expression::Taskgroup(expression) => collect_from_block(&expression.body, locals),
        Expression::Unary(expression) => {
            if expression.operator == Kind::Amp
                && let Some(name) = address_root_local(&expression.inner)
            {
                locals.insert(name.to_string());
            }
            collect_from_expression(&expression.inner, locals);
        }
        Expression::Binary(expression) => {
            collect_from_expression(&expression.left, locals);
            collect_from_expression(&expression.right, locals);
        }
        Expression::Group(expression) => collect_from_expression(&expression.inner, locals),
        Expression::Selector(expression) => collect_from_expression(&expression.inner, locals),
        Expression::Index(expression) => {
            collect_from_expression(&expression.inner, locals);
            collect_from_expression(&expression.index, locals);
        }
        Expression::Slice(expression) => {
            collect_from_expression(&expression.inner, locals);
            if let Some(start) = &expression.start {
                collect_from_expression(start, locals);
            }
            if let Some(end) = &expression.end {
                collect_from_expression(end, locals);
            }
        }
        Expression::StructLiteral(expression) => {
            for field in &expression.fields {
                collect_from_expression(&field.value, locals);
            }
        }
        Expression::ArrayLiteral(expression) => {
            for element in &expression.elements {
                collect_from_expression(element, locals);
            }
        }
        Expression::SliceLiteral(expression) => {
            for element in &expression.elements {
                collect_from_expression(element, locals);
            }
        }
        Expression::MapLiteral(expression) => {
            for pair in &expression.pairs {
                collect_from_expression(&pair.key, locals);
                collect_from_expression(&pair.value, locals);
            }
        }
        Expression::Propagate(expression) => collect_from_expression(&expression.inner, locals),
        Expression::Handle(expression) => {
            collect_from_expression(&expression.inner, locals);
            collect_from_block(&expression.handler, locals);
        }
        Expression::Ident(_)
        | Expression::Int(_)
        | Expression::Char(_)
        | Expression::String(_)
        | Expression::Bool(_)
        | Expression::Nil(_)
        | Expression::Error(_)
        | Expression::Missing(_) => {}
    }
}

fn address_root_local(expression: &Expression) -> Option<&str> {
    match expression {
        Expression::Ident(expression) => Some(&expression.name),
        Expression::Group(expression) => address_root_local(&expression.inner),
        Expression::Selector(expression) => address_root_local(&expression.inner),
        Expression::Index(expression) => address_root_local(&expression.inner),
        Expression::Unary(expression) if expression.operator == Kind::Star => None,
        _ => None,
    }
}
