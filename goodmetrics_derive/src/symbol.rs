use std::fmt::{Display, Formatter};

use syn::{Ident, Path};

pub const COMPONENT: Symbol = Symbol("component");
pub const HISTOGRAM: Symbol = Symbol("histogram");
pub const MONOTONIC: Symbol = Symbol("monotonic");
pub const NUMBER: Symbol = Symbol("number");
pub const STAT: Symbol = Symbol("stat");
pub const SUM: Symbol = Symbol("sum");

// Symbol strategy borrowed from serde_derive
#[derive(Copy, Clone)]
pub struct Symbol(&'static str);

impl PartialEq<Symbol> for Ident {
    fn eq(&self, word: &Symbol) -> bool {
        self == word.0
    }
}

impl PartialEq<Symbol> for &Ident {
    fn eq(&self, word: &Symbol) -> bool {
        *self == word.0
    }
}

impl PartialEq<Symbol> for Path {
    fn eq(&self, word: &Symbol) -> bool {
        self.is_ident(word.0)
    }
}

impl PartialEq<Symbol> for &Path {
    fn eq(&self, word: &Symbol) -> bool {
        self.is_ident(word.0)
    }
}

impl Display for Symbol {
    fn fmt(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
