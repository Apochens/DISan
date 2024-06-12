use tree_sitter::Node;
use colored::Colorize;

pub trait AstNode {
    fn to_source(&self, code: &str) -> String;
    fn dump_ast(&self);
    fn dump_source(&self, code: &str);

    fn is_header_include(&self) -> bool;
    fn is_using_declaration(&self) -> bool;
    fn is_function_definition(&self) -> bool;
}

impl<'tree> AstNode for Node<'tree> {
    fn to_source(&self, code: &str) -> String {
        (&code[self.start_byte()..self.end_byte()]).to_string()
    }
    fn dump_ast(&self) {
        println!("{}", self.to_sexp());
    }
    fn dump_source(&self, code: &str) {
        println!("{} ({}): {}",
            self.start_position().row
                .to_string().red().bold(),
            self.kind().green().bold(),
            self.to_source(code).split("\n")
                .map(|s| s.trim()).collect::<Vec<&str>>().join(" "),
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