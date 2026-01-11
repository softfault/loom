#![allow(unused)]
use std::collections::HashMap;

#[repr(transparent)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(usize);

#[derive(Default, Debug, Clone)]
pub struct Interner {
    /// string -> id
    map: HashMap<String, Symbol>,
    /// id -> string
    vec: Vec<String>,
}

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, name: &str) -> Symbol {
        if let Some(&sym) = self.map.get(name) {
            return sym;
        }

        let sym = unsafe { std::mem::transmute(self.vec.len()) };
        let name_string = name.to_string();
        self.vec.push(name_string.clone());
        self.map.insert(name_string, sym);
        sym
    }

    pub fn resolve(&self, sym: Symbol) -> &str {
        &self.vec[sym.0 as usize]
    }
}
