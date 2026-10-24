use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    File,
    Class,
    Interface,
    Enum,
    Constructor,
    Method,
    Function,
    Field,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::File => "File",
            EntityKind::Class => "Class",
            EntityKind::Interface => "Interface",
            EntityKind::Enum => "Enum",
            EntityKind::Constructor => "Constructor",
            EntityKind::Method => "Method",
            EntityKind::Function => "Function",
            EntityKind::Field => "Field",
        }
    }
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DepKind {
    Import,
    Extend,
    Implement,
    Call,
    Create,
    Override,
    Use,
    Parameter,
    Return,
    Annotation,
    Throw,
    Cast,
}

impl DepKind {
    pub fn from_capture(s: &str) -> Option<Self> {
        match s {
            "Import" => Some(DepKind::Import),
            "Extend" => Some(DepKind::Extend),
            "Implement" => Some(DepKind::Implement),
            "Call" => Some(DepKind::Call),
            "Create" => Some(DepKind::Create),
            "Override" => Some(DepKind::Override),
            "Use" => Some(DepKind::Use),
            "Parameter" => Some(DepKind::Parameter),
            "Return" => Some(DepKind::Return),
            "Annotation" => Some(DepKind::Annotation),
            "Throw" => Some(DepKind::Throw),
            "Cast" => Some(DepKind::Cast),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DepKind::Import => "Import",
            DepKind::Extend => "Extend",
            DepKind::Implement => "Implement",
            DepKind::Call => "Call",
            DepKind::Create => "Create",
            DepKind::Override => "Override",
            DepKind::Use => "Use",
            DepKind::Parameter => "Parameter",
            DepKind::Return => "Return",
            DepKind::Annotation => "Annotation",
            DepKind::Throw => "Throw",
            DepKind::Cast => "Cast",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Entity {
    pub var_name: String,
    pub name: String,
    pub kind: EntityKind,
    pub line_start: u32,
    pub line_end: u32,
    pub file_uri: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub src: usize,
    pub tgt: usize,
    pub kind: DepKind,
}
