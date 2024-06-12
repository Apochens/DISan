use std::collections::HashSet;
use tree_sitter::{Node, Parser};

use crate::edit::{Edit, EditConstant, EditKind};
use crate::matcher::{ConstructKind, DebugLocUpdateKind, FuncMatch};
use crate::traverse::{get_children_of_kind, get_fn_identifier, get_parent_of_kind, get_var_name_from_assign, get_var_name_from_decl};
use crate::ast::AstNode;

pub struct Instrumenter {
    parser: Parser,

    edits: Vec<Edit>,
    edit_track: HashSet<String>,

    header_include_edited: bool,
    global_var_decl_edited: bool,

    instr_file_name: String,
}

impl<'tree> Instrumenter {
    pub fn new(instr_file_name: String) -> Self {
        let mut parser = Parser::new();
        let grammar = tree_sitter_cpp::language();
        parser.set_language(&grammar).expect("Error loading grammar");

        Self { 
            parser,

            edits: vec![],
            edit_track: HashSet::new(),

            header_include_edited: false,
            global_var_decl_edited: false,

            instr_file_name,
        }
    }

    fn add_insert(&mut self, insert_str: String, insert_pos: usize) {
        let edit_hash = insert_pos.to_string() + &insert_str;
        if !self.edit_track.contains(&edit_hash) {
            self.edits.push(Edit::new_insert(insert_str, insert_pos));
            self.edit_track.insert(edit_hash);
        }
    }

    fn add_replace(&mut self, replace_str: String, start_pos: usize, end_pos: usize) {
        let edit_hash = start_pos.to_string() + &replace_str;
        if !self.edit_track.contains(&edit_hash) {
            self.edits.push(Edit::new_replace(replace_str, start_pos, end_pos));
            self.edit_track.insert(edit_hash);
        }
    }

    fn collect_header_include_edit(&mut self, header_include: &Node) {
        let insert_str = EditConstant::header_include_str();
        self.add_insert(insert_str, header_include.start_byte());

        self.header_include_edited = true;
    }

    fn collect_global_val_decl_edit(&mut self, using_decl: &Node) {
        if let Some(ud) = using_decl.next_sibling() {
            if ud.kind() != "using_declaration" {
                let insert_str = EditConstant::global_var_decl_str();
                self.add_insert(insert_str, using_decl.end_byte() + 1);

                self.global_var_decl_edited = true;
            }
        }
    }

    fn collect_init_and_clean_up_edit(&mut self, pass_entry: &Node, code: &String) {
        /* Check the parameter list */
        let param_list = pass_entry
            .child_by_field_name("declarator").unwrap()
            .child_by_field_name("parameters").unwrap();
        let params = get_children_of_kind(&param_list, "parameter_declaration");
        assert_eq!(params.len(), 2, "The pass entry should have two parameters!"); 

        let function_ptr = params[0].child_by_field_name("declarator").unwrap().child(1).unwrap();
        let function_analysis_manager = params[1].child_by_field_name("declarator").unwrap();
        let params = if function_analysis_manager.child_count() == 1 {
            /* xxxPass::run(Function &F, FunctionAnalysisManager &); */
            self.add_insert("AM".to_string(), function_analysis_manager.end_byte());
            vec![function_ptr.to_source(code), "AM".to_string()]
        } else {
            /* xxxPass::run(Function &F, FunctionAnalysisManager &AM); */
            vec![function_ptr.to_source(code), function_analysis_manager.child(1).unwrap().to_source(code)]
        };

        /* Instrument */
        let fn_body = pass_entry.child_by_field_name("body").unwrap();
        let init_str = format!(
            "RC = new RuntimeChecker({}, {}, \"{}\");\n  ",
            params[0],
            params[1],
            &self.instr_file_name
        );
        self.add_insert(init_str, fn_body.child(1).unwrap().start_byte());

        let return_stmts = get_children_of_kind(&fn_body, "return_statement");
        for return_stmt in return_stmts {
            let insert_str = format!("{{ RC->startCheck(); delete RC; ");
            self.add_insert(insert_str, return_stmt.start_byte());
            let insert_str = format!(" }}");
            self.add_insert(insert_str, return_stmt.end_byte());
        }
    }

    /// The S-expr of `Var->func(Args);` is shown as following:
    ///   (call_expression 
    ///        function: (field_expression 
    ///            argument: (identifier)    ==> `Var`
    ///            field: (field_identifier) ==> `func`
    ///        ) 
    ///        arguments: (argument_list)    ==> `Args`
    ///    ) 
    fn handle_call_to_field_expression(&mut self, call: &Node, code: &String) {
        assert_eq!(call.kind(), "call_expression");

        let function = call.child_by_field_name("function").unwrap();
        let arguments = call.child_by_field_name("arguments").unwrap();
        
        let fn_name = function.child_by_field_name("field").unwrap();
        let fn_name_str = fn_name.to_source(code);

        match fn_name_str.is_creation() {
            /* auto *NI = OI->clone(); */
            Some(ConstructKind::Cloning) => {
                if let Some(parent_decl) = get_parent_of_kind(&call, "declaration") {
                    let original_inst = function.child_by_field_name("argument").unwrap();
                    let var_name = get_var_name_from_decl(&parent_decl);
                    let insert_str = format!(
                        " RC->trackDebugLocDst({}, {}, {}, {}, \"{}\", \"{}\");",
                        var_name.to_source(&code),
                        original_inst.to_source(code),
                        ConstructKind::Cloning,
                        parent_decl.start_position().row,
                        original_inst.to_source(code),
                        var_name.to_source(&code),
                    );

                    self.add_insert(insert_str, parent_decl.end_byte());
                } else {
                    panic!("Cloned instruction without residing variable!");
                }
            },
            /* I->moveBefore(D, ..); */
            Some(ConstructKind::Moving) => {
                let debugloc_dst = function.child_by_field_name("argument").unwrap();
                if arguments.child_count() == 3 {
                    let move_dst = arguments.child(1).unwrap();
                    let insert_str = format!(
                        "{{ RC->trackDebugLocDst({}, {}, {}, {}, \"{}\", \"{}\"); ",
                        debugloc_dst.to_source(&code),  // DivInst
                        move_dst.to_source(&code),   // PreBB->getTerminater()
                        ConstructKind::Moving,
                        call.start_position().row,
                        debugloc_dst.to_source(&code),
                        move_dst.to_source(&code),
                    );

                    self.add_insert(insert_str, call.start_byte());
                    let insert_str = " }".to_string();
                    self.add_insert(insert_str, call.end_byte() + 1);
                } else if arguments.child_count() == 5 {
                    let move_dst_iter = arguments.child(3).unwrap();
                    let insert_str = format!(
                        "{{ RC->trackDebugLocDst({}, {}, {}, {}, \"{}\", \"{}\"); ", 
                        debugloc_dst.to_source(code),
                        move_dst_iter.to_source(code),
                        ConstructKind::Moving,
                        call.start_position().row,
                        debugloc_dst.to_source(code),
                        move_dst_iter.to_source(code),
                    );

                    self.add_insert(insert_str, call.start_byte());
                    let insert_str = format!(" }}");
                    self.add_insert(insert_str, call.end_byte() + 1);                    
                }
            },
            _ => {},
        };

        if fn_name_str.is_replacement() {
            /* The S-expr of `DLS->replaceAllUsesWith(DLD)` is shown as following:
             *  (call_expression 
             *       function: (field_expression 
             *           argument: (identifier) 
             *           field: (field_identifier)
             *       ) 
             *       arguments: (argument_list 
             *           (identifier)
             *       )
             *  )
             */

            let debugloc_src = function.child_by_field_name("argument").unwrap();
            let debugloc_src_str = debugloc_src.to_source(code);
            let debugloc_dst = arguments.child(1).unwrap(); // Only one arg
            let debugloc_dst_str = debugloc_dst.to_source(code).split("\n").map(|s| s.trim()).collect::<Vec<&str>>().join(" ");

            let prepare_str = format!(
                "Value *DebugLocSrc = {}; Value *DebugLocDst = {};",
                debugloc_src_str,
                debugloc_dst_str,
            );

            let replace_str = "DebugLocSrc->replaceAllUsesWith(DebugLocDst);".to_string();
            let hook_str = format!(
                "RC->trackDebugLocSrc(DebugLocDst, DebugLocSrc, {}, \"{}\", \"{}\");", 
                call.start_position().row,
                debugloc_dst_str,
                debugloc_src_str,
            );

            let replace_str = format!("{{ {} {} {} }}", prepare_str, replace_str, hook_str);
            self.add_replace(replace_str, call.start_byte(), call.end_byte() + 1);
        }

        match fn_name_str.is_debugloc_update() {
            Some(DebugLocUpdateKind::Preserving) => {
                // call.dump_source(code);

                let debugloc_dst = function.child_by_field_name("argument").unwrap();
                let debugloc = arguments.child(1).unwrap();
                if debugloc.kind() == "call_expression" {  // DLD->setDebugLoc(DLS->getDebugLoc())
                    // let debugloc_src = debugloc.child_by_field_name("function").unwrap().child_by_field_name("argument").unwrap();
                    // debugloc_src.dump_source(code);
                    
                    let insert_str = format!("{{ ");
                    self.add_insert(insert_str, call.start_byte());

                    let insert_str = format!(
                        " RC->trackDebugLocUpdate({}, nullptr, {}, {}, \"{}\", \"nullptr\"); }}",
                        debugloc_dst.to_source(code),
                        // debugloc_src.to_source(code),
                        DebugLocUpdateKind::Preserving,
                        call.start_position().row,
                        debugloc_dst.to_source(code),
                        // debugloc_src.to_source(code),
                    );

                    self.add_insert(insert_str, call.end_byte() + 1);
                } else {
                    panic!("Encounter dbeug location update using variable directly!");
                }

            },
            Some(DebugLocUpdateKind::Merging) => {

            },
            Some(DebugLocUpdateKind::Dropping) => {

            },
            None => {},
        };

        if fn_name_str.is_insertion() {
            let debugloc_dst = function.child_by_field_name("argument").unwrap();
            if arguments.child_count() == 3 {
                let inst_insert_pos = arguments.child(1).unwrap();

                let insert_str = format!(
                    "{{ RC->trackInsertion({}, {}, {}, \"{}\", \"{}\"); ",
                    debugloc_dst.to_source(code),
                    inst_insert_pos.to_source(code),
                    call.start_position().row,
                    debugloc_dst.to_source(code),
                    inst_insert_pos.to_source(code),
                );
                self.add_insert(insert_str, call.start_byte());

                let insert_str = format!(" }}");
                self.add_insert(insert_str, call.end_byte() + 1);
            }
        }
    }

    /* BinaryOperator::CreateMul */
    fn handle_call_to_qualified_identifier(&mut self, call: &Node, code: &String) {
        assert_eq!(call.kind(), "call_expression");

        let function = call.child_by_field_name("function").unwrap();
        let arguments = call.child_by_field_name("arguments").unwrap();

        let fn_name_str = function.to_source(code);
        if let Some(ConstructKind::Creating) = fn_name_str.is_creation() {

            if let Some(parent_decl) = get_parent_of_kind(&call, "declaration") {
                let var_name = get_var_name_from_decl(&parent_decl);
                let insert_str = format!(
                    " RC->trackDebugLocDst({}, nullptr, {}, {}, \"{}\", \"\");",
                    var_name.to_source(code),
                    ConstructKind::Creating,
                    parent_decl.start_position().row,
                    var_name.to_source(code),
                );
                self.add_insert(insert_str, parent_decl.end_byte());
            }

            if let Some(parent_assign) = get_parent_of_kind(&call, "assignment_expression") {
                let var_name = get_var_name_from_assign(&parent_assign);
                let insert_str = format!(
                    " RC->trackDebugLocDst({}, nullptr, {}, {}, \"{}\", \"\");",
                    var_name.to_source(code),
                    ConstructKind::Creating,
                    parent_assign.start_position().row,
                    var_name.to_source(code),
                );
                self.add_insert(insert_str, parent_assign.end_byte() + 1);
            }
        }
    }

    fn handle_call_to_identifier(&mut self, call: &Node, code: &String) {
        assert_eq!(call.kind(), "call_expression");

    }

    fn collect_fn_body_edit(&mut self, fn_def: &Node, code: &String) {
        assert_eq!(fn_def.kind(), "function_definition");

        let body = fn_def.child_by_field_name("body").unwrap();
        let calls = get_children_of_kind(&body, "call_expression");
        for call in calls {
            let function = call.child_by_field_name("function").unwrap();

            match function.kind() {
                /* Sub->insertAfter: Move, Clone, Replacement, DebugLocUpdate */
                "field_expression" => self.handle_call_to_field_expression(&call, code),
                /* BinaryOperator::CreateMul */
                "qualified_identifier" => self.handle_call_to_qualified_identifier(&call, code),
                /* onlyNameFuncion */
                "identifier" => self.handle_call_to_identifier(&call, code),
                "template_function" => {},
                _ => {},
            }
        }
    }

    fn collect_fn_edit(&mut self, fn_def: &Node, code: &String) {        
        let fn_name = get_fn_identifier(fn_def).to_source(&code);

        if fn_name.is_pass_entry() {  /* Insert main entry */
            self.collect_init_and_clean_up_edit(fn_def, code);
        } else {  /* Insert nomral functions */
            self.collect_fn_body_edit(fn_def, code);
        }
    }

    /// Collect modifications on the given AST
    fn collect_edits(&mut self, buf: &String) {

        let tree = self.parser.parse(&buf, None).unwrap();
        let root_node = tree.root_node(); 
        
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
    
            // Header file insertion
            if child.is_header_include() && !self.header_include_edited {
                self.collect_header_include_edit(&child);
            }
    
            // Global variable insertion
            if child.is_using_declaration() && !self.global_var_decl_edited {
                self.collect_global_val_decl_edit(&child);
            }
    
            // Main insertion
            if child.is_function_definition() {
                self.collect_fn_edit(&child, buf);
            }
        }
    }

    /// The main function to perform AST-level instrumentation
    pub fn instrument(&mut self, buf: &mut String) {
        self.collect_edits(buf);

        self.edits.sort_by(|a, b| b.start_pos.cmp(&a.start_pos));
        for edit in &self.edits {
            match edit.kind {
                EditKind::Insert => {
                    buf.insert_str(edit.start_pos, &edit.content);
                },
                EditKind::Replace(end_pos) => {
                    buf.replace_range(edit.start_pos..end_pos, &edit.content);
                },
            }
        }
    }
}

