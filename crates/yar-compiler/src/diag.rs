use crate::{symbol::sanitize_diagnostic_message, token::Position};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub pos: Position,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct List {
    items: Vec<Diagnostic>,
}

impl List {
    pub fn add(&mut self, pos: Position, message: impl Into<String>) {
        self.items.push(Diagnostic {
            pos,
            message: sanitize_diagnostic_message(message.into()),
        });
    }

    pub fn append(&mut self, other: &[Diagnostic]) {
        self.items
            .extend(other.iter().cloned().map(|mut diagnostic| {
                diagnostic.message = sanitize_diagnostic_message(diagnostic.message);
                diagnostic
            }));
    }

    pub fn items(&self) -> Vec<Diagnostic> {
        self.items.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

pub fn format(path: &str, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let diag_path = if diagnostic.pos.file.is_empty() {
                path
            } else {
                &diagnostic.pos.file
            };
            format!(
                "{}:{}:{}: {}",
                diag_path, diagnostic.pos.line, diagnostic.pos.column, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
