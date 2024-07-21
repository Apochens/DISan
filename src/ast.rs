use tree_sitter::Node;
use colored::Colorize;

pub trait AstNode {
    fn row(&self) -> usize;

    fn to_raw(&self, code: &str) -> String;
    fn to_source(&self, code: &str) -> String;
    fn dump_ast(&self);
    fn dump_source(&self, code: &str);

    fn is_header_include(&self) -> bool;
    fn is_using_declaration(&self) -> bool;
    fn is_function_definition(&self) -> bool;
}

impl<'tree> AstNode for Node<'tree> {
    fn row(&self) -> usize {
        self.start_position().row + 1
    }

    fn to_raw(&self, code: &str) -> String {
        code[self.start_byte()..self.end_byte()].to_string()
    }

    fn to_source(&self, code: &str) -> String {
        let source: Vec<&str> = (&code[self.start_byte()..self.end_byte()]).split("\n").map(|s| s.trim()).collect();
        source.join(" ")
    }

    fn dump_ast(&self) {
        println!("{}", self.to_sexp());
    }

    fn dump_source(&self, code: &str) {
        println!("{} ({}): {}",
            self.start_position().row.to_string().red().bold(),
            self.kind().green().bold(),
            self.to_raw(code),
        );
    }

    fn is_function_definition(&self) -> bool {
        self.kind() == "function_definition"
    }
    fn is_header_include(&self) -> bool {
        self.kind() == "preproc_include"
    }
    fn is_using_declaration(&self) -> bool {
        self.kind() == "using_declaration"
    }
}

pub enum ASTNodeKind {
    HeaderInclude,
    UsingDecl,
    FnDef,
    CallExpr,
    NewExpr,
    FieldExpr,
}

impl ASTNodeKind {
    pub fn to_string(&self) -> &'static str {
        match self {
            ASTNodeKind::HeaderInclude => "preproc_include",
            ASTNodeKind::UsingDecl => "using_declaration",
            ASTNodeKind::FnDef => "function_definition",
            ASTNodeKind::CallExpr => "call_expression",
            ASTNodeKind::NewExpr => "new_expression",
            ASTNodeKind::FieldExpr => "field_expression",
        }
    }
}

impl Into<&str> for ASTNodeKind {
    fn into(self) -> &'static str {
        self.to_string()
    }
}

/* Decrepeted */
// pub fn to_ast_node<'tree>(node: Node<'tree>, code: &'tree str) -> ANode<'tree> {
//     ANode::new(node, code)
// }

// /// `ASTNode` wraps `Node` to represents the AST node
// pub struct ANode<'tree> {
//     node: Node<'tree>,
//     code: &'tree str,
//     content: String,
// }

// impl<'tree> ANode<'tree> {
//     pub fn new(node: Node<'tree>, code: &'tree str) -> Self {
//         Self {
//             node,
//             code,
//             content: (&code[node.start_byte()..node.end_byte()]).to_string()
//         }
//     }

//     pub fn content(&self) -> &str {
//         &self.content
//     }
// }

// /* Wrappers */
// impl<'tree> ANode<'tree> {
//     pub fn start(&self) -> usize {
//         self.node.start_byte()
//     }

//     pub fn end(&self) -> usize {
//         self.node.end_byte()
//     }

//     pub fn start_row(&self) -> usize {
//         self.node.start_position().row
//     }

//     pub fn child_count(&self) -> usize {
//         self.node.child_count()
//     }

//     pub fn child(&self, i: usize) -> Option<ANode> {
//         if let Some(node) = self.node.child(i) {
//             Some(to_ast_node(node, self.code))
//         } else {
//             None
//         }
//     }

//     pub fn child_by_field_name<T: AsRef<[u8]>>(&self, field_name: T) -> Option<ANode> {
//         if let Some(node) = self.node.child_by_field_name(field_name) {
//             Some(to_ast_node(node, self.code))
//         } else {
//             None
//         }
//     }

//     pub fn kind(&self) -> &str {
//         self.node.kind()
//     }
// }

// impl Display for ANode<'_> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", self.content)
//     }
// }

// /* AST node kind checks */
// impl ANode<'_> {
//     pub fn is_function_definition(&self) -> bool {
//         self.node.kind() == "function_definition"
//     }

//     pub fn is_header_include(&self) -> bool {
//         self.node.kind() == "preproc_include"
//     }

//     pub fn is_using_declaration(&self) -> bool {
//         self.node.kind() == "using_declaration"
//     }

//     pub fn is_call_expression(&self) -> bool {
//         self.node.kind() == "call_expression"
//     }

//     pub fn is_new_expression(&self) -> bool {
//         self.node.kind() == "new_expression"
//     }

//     pub fn is_pass_entry(&self) -> bool {
//         self.content.ends_with("Pass::run")
//     }
// }

// pub enum ExprKind {
//     Constructor(ConstructKind),
//     DLUpdateOperator(DLUpdateKind),
//     ReplaceFn,
//     InsertFn,
// }

// const CONSTUCTOR_CREATE: [&str; 1] = [
//     "BinaryOperator::Create",
// ];

// const CONSTRUCTOR_CLONE: [&str; 1] = [
//     "clone",
// ];

// const CONSTRUCTOR_MOVE: [&str; 2] = [
//     "moveBefore",
//     "moveAfter",
// ];

// const REPLACE_FN: [&str; 1] = [
//     "replaceAllUsesWith",
// ];

// const INSERT_FN: [&str; 2] = [
//     "insertBefore",
//     "insertAfter",
// ];

// impl ANode<'_> {
//     pub fn expr_kind(&self) -> Option<ExprKind> {
//         for prefix in CONSTUCTOR_CREATE {
//             if self.content.starts_with(prefix) {
//                 return Some(ExprKind::Constructor(ConstructKind::Creating));
//             }
//         }

//         for prefix in CONSTRUCTOR_CLONE {
//             if self.content.starts_with(prefix) {
//                 return Some(ExprKind::Constructor(ConstructKind::Cloning));
//             }
//         }

//         for prefix in CONSTRUCTOR_MOVE {
//             if self.content.starts_with(prefix) {
//                 return Some(ExprKind::Constructor(ConstructKind::Moving));
//             }
//         }

//         for prefix in REPLACE_FN {
//             if self.content.starts_with(prefix) {
//                 return Some(ExprKind::ReplaceFn);
//             }
//         }

//         for prefix in INSERT_FN {
//             if self.content.starts_with(prefix) {
//                 return Some(ExprKind::InsertFn);
//             }
//         }


        
//         None
//     }
// }