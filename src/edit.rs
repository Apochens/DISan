#[derive(PartialEq)]
pub enum EditKind {
    Insert,
    Replace(usize),
}

pub struct Edit {
    pub content: String,
    pub start_pos: usize,
    pub kind: EditKind,
}

impl Edit {
    pub fn new_insert(insert_str: String, insert_pos: usize) -> Self {
        Self {
            content: insert_str,
            start_pos: insert_pos,
            kind: EditKind::Insert,
        }
    }

    pub fn new_replace(replace_str: String, start_pos: usize, end_pos: usize) -> Self {
        Self {
            content: replace_str,
            start_pos,
            kind: EditKind::Replace(end_pos),
        }
    }
}

pub struct EditConstant;

impl EditConstant {
    pub fn header_include_str() -> String {
        "#include \"llvm/Transforms/Utils/RuntimeDLChecker.h\"\n".to_string()
    }

    pub fn global_var_decl_str() -> String {
        "namespace { RuntimeChecker *RC = nullptr; }\n".to_string()
    }
}