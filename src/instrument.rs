use std::collections::HashSet;
use tree_sitter::{Node, Parser};

use crate::edit::{Edit, EditKind};
use crate::hook::{Hook, HookEnv};
use crate::matcher::{ConstructKind, DLUpdateKind, FuncMatch};
use crate::traverse::{get_children_of_kind, get_fn_identifier, get_ident_from_call, get_parent_of_kind, get_var_name_from_assign, get_var_name_from_decl};
use crate::ast::{ASTNodeKind, AstNode};

pub struct Instrumenter {
    parser: Parser,

    edits: Vec<Edit>,
    edit_track: HashSet<String>,

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

    fn collect_init_and_clean_up_edit(&mut self, pass_entry: &Node, code: &str) {
        /* Check the parameter list */
        let param_list = pass_entry
            .child_by_field_name("declarator").unwrap()
            .child_by_field_name("parameters").unwrap();
        let params = get_children_of_kind(&param_list, "parameter_declaration");
        assert!(params.len() >= 2, "The pass entry should have two parameters!"); 

        let pass_target_type = params[0].child_by_field_name("type").unwrap();
        let pass_target_type_str = pass_target_type.to_source(code);

        let params = match pass_target_type_str.as_str() {
            "Function" => {
                let function_ptr = params[0].child_by_field_name("declarator").unwrap().child(1).unwrap();
                let function_analysis_manager = params[1].child_by_field_name("declarator").unwrap();
                if function_analysis_manager.child_count() == 1 {
                    /* xxxPass::run(Function &F, FunctionAnalysisManager &); */
                    self.add_insert("AM".to_string(), function_analysis_manager.end_byte());
                    vec![function_ptr.to_source(code), "AM".to_string()]
                } else {
                    /* xxxPass::run(Function &F, FunctionAnalysisManager &AM); */
                    vec![function_ptr.to_source(code), function_analysis_manager.child(1).unwrap().to_source(code)]
                }
            },
            "Loop" => {
                let loop_ptr = params[0].child_by_field_name("declarator").unwrap().child(1).unwrap();
                let loop_standard_analysis_results = params[2].child_by_field_name("declarator").unwrap();
                if loop_standard_analysis_results.child_count() == 1 {
                    self.add_insert("AM".to_string(), loop_standard_analysis_results.end_byte());
                    vec![loop_ptr.to_source(code), "AR".to_string()]
                } else {
                    vec![loop_ptr.to_source(code), loop_standard_analysis_results.child(1).unwrap().to_source(code)]
                }
            },
            _ => vec![],
        };

        if params.is_empty() {
            return ;
        }

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


    /// The main function to perform AST-level instrumentation
    pub fn instrument(&mut self, buf: &mut String) {
        // self.collect_edits(buf);
        self.visit_ast_tree(buf);

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

impl Instrumenter {
    fn visit_header_includes(&mut self, nodes: Vec<Node>) {
        assert_eq!(nodes.is_empty(), false, "No header includes in the code!");
        self.add_insert(
            HookEnv::header_include_str(), 
            nodes[0].start_byte()
        );
    }

    fn visit_using_decls(&mut self, nodes: Vec<Node>) {
        assert_eq!(nodes.is_empty(), false, "No using declaration in the code!");
        self.add_insert(
            HookEnv::global_var_decl_str(), 
            nodes[0].end_byte() + 1
        );
    }

    fn visit_fn_calls(&mut self, nodes: Vec<Node>, code: &str) {
        for call in nodes {
            let callee = call.child_by_field_name("function").unwrap();
            let arguments = call.child_by_field_name("arguments").unwrap();

            /* Distinguish `->` (field_expr) and `::` (qualified_ident) */
            let mut callee_name = callee.to_source(code);
            match callee.kind() {
                "field_expression" => {
                    callee_name = callee.child_by_field_name("field").unwrap().to_source(code);
                },
                "qualified_identifier" => {},
                _ => continue,
            };

            match callee_name.is_construct() {
                Some(ConstructKind::Creating) => {
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
                        continue;
                    }
        
                    if let Some(parent_assign) = get_parent_of_kind(&call, "assignment_expression") {
                        let var_name = get_var_name_from_assign(&parent_assign);
        
                        let insert_str = format!("{{ ");
                        self.add_insert(insert_str, parent_assign.start_byte());
        
                        let insert_str = format!(
                            " RC->trackDebugLocDst({}, nullptr, {}, {}, \"{}\", \"\"); }}",
                            var_name.to_source(code),
                            ConstructKind::Creating,
                            parent_assign.start_position().row,
                            var_name.to_source(code),
                        );
                        self.add_insert(insert_str, parent_assign.end_byte() + 1);
                        continue;
                    }
        
                    if let Some(parent_return) = get_parent_of_kind(&call, "return_statement") {
                        let replace_str = format!(
                            "{{ auto *V = {}; RC->trackDebugLocDst(V, nullptr, {}, {}, \"\", \"\"); return V; }}", 
                            call.to_source(code),
                            ConstructKind::Creating,
                            call.start_position().row,
                        );
        
                        self.add_replace(replace_str, parent_return.start_byte(), parent_return.end_byte());
                        continue;
                    }
                },
                /* auto *NI = OI->clone(); */
                Some(ConstructKind::Cloning) => {
                    let original_inst = callee.child_by_field_name("argument").unwrap();
                    if let Some(parent_decl) = get_parent_of_kind(&call, "declaration") {
                        let var_name = get_var_name_from_decl(&parent_decl);
                        let insert_str = format!(
                            " RC->trackDebugLocDst({}, {}, {}, {}, \"{}\", \"{}\");",
                            var_name.to_source(&code),
                            original_inst.to_source(code),
                            ConstructKind::Cloning,
                            parent_decl.start_position().row,
                            var_name.to_source(&code),
                            original_inst.to_source(code),
                        );

                        self.add_insert(insert_str, parent_decl.end_byte());
                        continue;
                    }

                    if let Some(parent_assign) = get_parent_of_kind(&call, "assignment_expression") {
                        let var_name = get_var_name_from_assign(&parent_assign);
                        let insert_str = format!(
                            " RC->trackDebugLocDst({}, {}, {}, {}, \"{}\", \"{}\");",
                            var_name.to_source(code),
                            original_inst.to_source(code),
                            ConstructKind::Cloning,
                            parent_assign.start_position().row,
                            var_name.to_source(code),
                            original_inst.to_source(code),
                        );

                        self.add_insert(insert_str, parent_assign.end_byte() + 1);
                        continue;
                    }

                    panic!("Failed to parse instruction clone!");
                },
                /* I->moveBefore(D, ..); */
                Some(ConstructKind::Moving) => {
                    let debugloc_dst = callee.child_by_field_name("argument").unwrap();
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
                        let insert_str = format!(" }}");
                        self.add_insert(insert_str, call.end_byte() + 1);
                        continue;
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
                        continue;                
                    }
                },
                None => {},
            };

            match callee_name.as_str() {
                /* OldI->replaceAllUsesWith(NewI) */
                "replaceAllUsesWith" => {
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
                    let debugloc_src = callee.child_by_field_name("argument").unwrap();
                    let debugloc_src_str = debugloc_src.to_source(code);
                    let debugloc_dst = arguments.child(1).unwrap(); // Only one arg
                    let debugloc_dst_str = debugloc_dst.to_source(code).split("\n").map(|s| s.trim()).collect::<Vec<&str>>().join(" ");
                    
                    // We need to distinguish between `Value &` (DLS.replace) and `Value *` (DLS->replace)
                    let field_operator = callee.child(1).unwrap().to_source(code);
                    let prepare_str = format!(
                        "Value {}DebugLocSrc = {}; Value *DebugLocDst = {};",
                        if field_operator.as_str() == "." { "&" } else {"*"},
                        debugloc_src_str,
                        debugloc_dst_str,
                    );

                    let replace_str = format!(
                        "DebugLocSrc{}replaceAllUsesWith(DebugLocDst);", 
                        &field_operator
                    );

                    let hook_str = format!(
                        "RC->trackDebugLocSrc(DebugLocDst, {}DebugLocSrc, {}, \"{}\", \"{}\");", 
                        if field_operator.as_str() == "." { "&" } else { "" },
                        call.start_position().row,
                        debugloc_dst_str,
                        debugloc_src_str,
                    );

                    let replace_str = format!("{{ {} {} {} }}", prepare_str, replace_str, hook_str);
                    self.add_replace(replace_str, call.start_byte(), call.end_byte() + 1);
                },
                /* I->replaceUsesOfWith(OldI, NewI); */
                "replaceUsesOfWith" => {

                },
                _ => {},
            }

            match callee_name.is_debugloc_update() {
                Some(DLUpdateKind::Preserving) => {
                    let debugloc_dst = callee.child_by_field_name("argument").unwrap();
                    let debugloc = arguments.child(1).unwrap();
                    let debugloc_src = if debugloc.kind() == "call_expression" {
                        get_ident_from_call(&debugloc, "getDebugLoc", code)
                    } else {
                        None
                    };

                    let insert_str = format!("{{ ");
                    self.add_insert(insert_str, call.start_byte());

                    let insert_str = format!(
                        " RC->trackDebugLocPreserving({}, nullptr, {}, \"{}\", \"nullptr\"); }}",
                        debugloc_dst.to_source(code),
                        call.start_position().row,
                        debugloc_dst.to_source(code),
                    );

                    self.add_insert(insert_str, call.end_byte() + 1);
                },
                Some(DLUpdateKind::Merging) => {
                    let debugloc_dst = callee.child_by_field_name("argument").unwrap();

                    let debugloc_1 = arguments.child(1).unwrap();
                    let debugloc_src_1 = if debugloc_1.kind() == "call_expression" {
                        get_ident_from_call(&debugloc_1, "getDebugLoc", code)
                    } else {
                        None
                    };
    
                    let debugloc_2 = arguments.child(3).unwrap();
                    let debugloc_src_2 = if debugloc_2.kind() == "call_expression" {
                        get_ident_from_call(&debugloc_2, "getDebugLoc", code)
                    } else {
                        None
                    };
                },
                Some(DLUpdateKind::Dropping) => {
                    let debugloc_dst = callee.child_by_field_name("argument").unwrap();

                    let insert_str = format!("{{ ");
                    self.add_insert(insert_str, call.start_byte());

                    let insert_str = format!(
                        " RC->trackDebugLocDropping({}, {}, \"{}\"); }}",
                        debugloc_dst.to_source(code),
                        call.start_position().row,
                        debugloc_dst.to_source(code),
                    );
                    self.add_insert(insert_str, call.end_byte() + 1);
                },
                None => {},
            };

            if callee_name.is_insertion() {
                let debugloc_dst = callee.child_by_field_name("argument").unwrap();
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
    }

    fn visit_new_exprs(&mut self, nodes: Vec<Node>, code: &str) {
        for new in nodes {
            let new_type = new.child_by_field_name("type").unwrap();
            let new_type_str = new_type.to_source(code);
            if let Some(ConstructKind::Creating) = new_type_str.is_construct() {
                if let Some(parent_decl) = get_parent_of_kind(&new, "declaration") {
                    let var_name = get_var_name_from_decl(&parent_decl);
                    let insert_str = format!(
                        " RC->trackDebugLocDst({}, nullptr, {}, {}, \"{}\", \"\");", 
                        var_name.to_source(code),
                        ConstructKind::Creating,
                        new.start_position().row,
                        var_name.to_source(code),
                    );

                    self.add_insert(insert_str, parent_decl.end_byte());
                    continue;
                }

                if let Some(parent_assign) = get_parent_of_kind(&new, "assignment_expression") {
                    let var_name = get_var_name_from_assign(&parent_assign);

                    let insert_str = format!("{{ ");
                    self.add_insert(insert_str, parent_assign.start_byte());

                    let insert_str = format!(
                        " RC->trackDebugLocDst({}, nullptr, {}, {}, \"{}\", \"\"); }}",
                        var_name.to_source(code),
                        ConstructKind::Creating,
                        new.start_position().row,
                        var_name.to_source(code),
                    );
                    
                    self.add_insert(insert_str, parent_assign.end_byte() + 1);
                    continue;
                }

                if let Some(parent_return) = get_parent_of_kind(&new, "return_statement") {
                    parent_return.dump_source(code);
                    unreachable!();
                }

                unreachable!();
            }
        }
    }

    fn visit_fn_defs(&mut self, nodes: Vec<Node>, code: &str) {
        for fn_def in nodes {
            let fn_ident = get_fn_identifier(&fn_def);
            if fn_ident.to_source(code).is_pass_entry() {
                /* Add initialization and clean up */
                self.collect_init_and_clean_up_edit(&fn_def, code);
            } else {
                /* Process all function calls */
                self.visit_fn_calls(get_children_of_kind(
                    &fn_def, 
                    ASTNodeKind::CallExpr.into()), code
                );
                /* Process all object news */
                self.visit_new_exprs(get_children_of_kind(
                    &fn_def, 
                    ASTNodeKind::NewExpr.into()), code
                );
            }
        }
    }

    fn visit_ast_tree(&mut self, code: &str) {
        let tree = self.parser.parse(code, None).expect("Failed to parse the code!");
        let root_node = tree.root_node();

        /* Instrument the header include */
        self.visit_header_includes(get_children_of_kind(
            &root_node, 
            ASTNodeKind::HeaderInclude.into()
        ));

        /* Instrument the global varaible */
        self.visit_using_decls(get_children_of_kind(
            &root_node, 
            ASTNodeKind::UsingDecl.into()
        ));

        /* Instrument the hooks */
        self.visit_fn_defs(get_children_of_kind(
            &root_node, 
            ASTNodeKind::FnDef.into()), code
        );
    }
}