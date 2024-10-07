use colored::Colorize;
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

use crate::ast::{ASTNodeKind, AstNode};
use crate::edit::{Edit, EditKind};
use crate::hook::Hook;
use crate::matcher::{ConstructKind, DLUpdateKind, FuncMatch};
use crate::traverse::{
    get_children_of_kind, get_fn_identifier, get_ident_from_call, get_parent_of_kind,
    get_var_name_from_assign, get_var_name_from_decl,
};

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
        parser
            .set_language(&grammar)
            .expect("Error loading grammar");

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
            self.edits
                .push(Edit::new_replace(replace_str, start_pos, end_pos));
            self.edit_track.insert(edit_hash);
        }
    }

    fn collect_init_and_clean_up_edit(&mut self, pass_entry: &Node, code: &str) {
        /* Check the parameter list */
        let param_list = pass_entry
            .child_by_field_name("declarator")
            .unwrap()
            .child_by_field_name("parameters")
            .unwrap();
        let params = get_children_of_kind(&param_list, "parameter_declaration");
        assert!(
            params.len() >= 1,
            "The pass entry should have the target parameters!"
        );

        let pass_target_type = params[0].child_by_field_name("type").unwrap();
        let pass_target_type_str = pass_target_type.to_source(code);

        let pass_target = match pass_target_type_str.as_str() {
            "Function" => {
                let function_ref = params[0]
                    .child_by_field_name("declarator")
                    .unwrap()
                    .child(1)
                    .unwrap();
                function_ref.to_source(code)
            }
            "Loop" => {
                let loop_ref = params[0]
                    .child_by_field_name("declarator")
                    .unwrap()
                    .child(1)
                    .unwrap();
                loop_ref.to_source(code)
            }
            "LoopNest" => {
                let loop_nest_ref = params[0]
                    .child_by_field_name("declarator")
                    .unwrap()
                    .child(1)
                    .unwrap();
                loop_nest_ref.to_source(code)
            }
            _ => unreachable!(),
        };

        /* Instrument */
        let fn_body = pass_entry.child_by_field_name("body").unwrap();
        let init_str = format!(
            "RC = new RuntimeChecker({}, \"{}\");\n  ",
            pass_target, &self.instr_file_name
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
                }
                EditKind::Replace(end_pos) => {
                    buf.replace_range(edit.start_pos..end_pos, &edit.content);
                }
            }
        }
    }
}

impl Instrumenter {
    fn visit_header_includes(&mut self, nodes: Vec<Node>) {
        assert_eq!(nodes.is_empty(), false, "No header includes in the code!");
        self.add_insert(Hook::header_include().to_string(), nodes[0].start_byte());
    }

    fn visit_using_decls(&mut self, nodes: Vec<Node>) {
        assert_eq!(nodes.is_empty(), false, "No using declaration in the code!");
        self.add_insert(Hook::global_var_decl().to_string(), nodes[0].end_byte() + 1);
    }

    fn try_visit_insertions(&mut self, call: Node, callee_name: &str, code: &str) {
        assert_eq!(call.kind(), "call_expression");

        let callee = call.child_by_field_name("function").unwrap();
        let arguments = call.child_by_field_name("arguments").unwrap();
        if callee.kind() != ASTNodeKind::FieldExpr.to_string() {
            return;
        }

        let inserted_inst = callee.child_by_field_name("argument").unwrap();
        let insert_pos = match callee_name {
            // 1 - void Instruction::insertBefore(BasicBlock::iterator InsertPos);
            // 2 - void Instruction::insertBefore(BasicBlock &BB, InstListType::iterator InsertPos);
            "insertBefore" => {
                match arguments.child_count() {
                    3 => {
                        // 1
                        let insert_it = arguments.child(1).unwrap();
                        format!("&*{}", insert_it.to_source(code))
                    }
                    5 => {
                        // 2
                        let insert_bb = arguments.child(3).unwrap();
                        format!("&{}", insert_bb.to_source(code))
                    }
                    _ => unreachable!(),
                }
            }
            // void Instruction::insertAfter(Instruction *InsertPos);
            "insertAfter" => arguments.child(1).unwrap().to_source(code),
            // BasicBlock::iterator Instruction::insertInto(BasicBlock *ParentBB, BasicBlock::iterator It);
            "insertInto" => arguments.child(1).unwrap().to_source(code),
            _ => {
                return;
            }
        };

        let field_op = callee.child(1).unwrap().to_source(code);

        let insert_str = format!(
            "{{ RC->trackInsertion({}{}, {}, {}, \"{}\", \"{}\"); ",
            if field_op.as_str() == "." { "&" } else { "" },
            inserted_inst.to_source(code),
            insert_pos,
            call.row(),
            inserted_inst.to_source(code),
            insert_pos,
        );
        self.add_insert(insert_str, call.start_byte());

        let insert_str = format!(" }}");
        self.add_insert(insert_str, call.end_byte() + 1);
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
                }
                "qualified_identifier" => {}
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
                            parent_decl.row(),
                            var_name.to_source(code),
                        );
                        self.add_insert(insert_str, parent_decl.end_byte());
                        continue;
                    }

                    if let Some(parent_assign) = get_parent_of_kind(&call, "assignment_expression")
                    {
                        let var_name = get_var_name_from_assign(&parent_assign);

                        let insert_str = format!("{{ ");
                        self.add_insert(insert_str, parent_assign.start_byte());

                        let insert_str = format!(
                            " RC->trackDebugLocDst({}, nullptr, {}, {}, \"{}\", \"\"); }}",
                            var_name.to_source(code),
                            ConstructKind::Creating,
                            parent_assign.row(),
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
                            call.row(),
                        );

                        self.add_replace(
                            replace_str,
                            parent_return.start_byte(),
                            parent_return.end_byte(),
                        );
                        continue;
                    }

                    if let Some(parent) = call.parent() {
                        if parent.kind() == "expression_statement" {
                            let replace_str = format!(
                                "Instruction *I = {}; RC->trackDebugLocDst(I, nullptr, {}, {}, \"\", \"\")",
                                call.to_source(code),
                                ConstructKind::Creating,
                                call.row(),
                            );
                            self.add_replace(replace_str, call.start_byte(), call.end_byte());
                        }
                    }
                }
                /* auto *NI = OI->clone(); */
                Some(ConstructKind::Cloning) => {
                    let original_inst = callee.child_by_field_name("argument").unwrap();
                    let addr_op = if callee.child(1).unwrap().to_source(code).as_str() == "->" {
                        ""
                    } else {
                        "&"
                    };
                    if let Some(parent_decl) = get_parent_of_kind(&call, "declaration") {
                        let var_name = get_var_name_from_decl(&parent_decl);

                        let insert_str = format!(
                            " RC->trackDebugLocDst({}, {}{}, {}, {}, \"{}\", \"{}\");",
                            var_name.to_source(&code),
                            addr_op,
                            original_inst.to_source(code),
                            ConstructKind::Cloning,
                            parent_decl.row(),
                            var_name.to_source(&code),
                            original_inst.to_source(code),
                        );
                        self.add_insert(insert_str, parent_decl.end_byte());

                        continue;
                    }

                    if let Some(parent_assign) = get_parent_of_kind(&call, "assignment_expression")
                    {
                        let var_name = get_var_name_from_assign(&parent_assign);

                        let insert_str = format!("{{ ");
                        self.add_insert(insert_str, parent_assign.start_byte());

                        let insert_str = format!(
                            " RC->trackDebugLocDst({}, {}{}, {}, {}, \"{}\", \"{}\"); }}",
                            var_name.to_source(code),
                            addr_op,
                            original_inst.to_source(code),
                            ConstructKind::Cloning,
                            parent_assign.row(),
                            var_name.to_source(code),
                            original_inst.to_source(code),
                        );
                        self.add_insert(insert_str, parent_assign.end_byte() + 1);

                        continue;
                    }

                    call.dump_source(code);
                    panic!("Failed to parse instruction clone!");
                }
                /* I->moveBefore(D, ..); */
                Some(ConstructKind::Moving) => {
                    let debugloc_dst = callee.child_by_field_name("argument").unwrap();
                    let move_dst = match arguments.child_count() {
                        3 => arguments.child(1).unwrap().to_source(code),
                        5 => {
                            format!("&{}", arguments.child(1).unwrap().to_source(code))
                        }
                        _ => unreachable!(),
                    };
                    let field_op = callee.child(1).unwrap().to_source(code);
                    let ref_op = if field_op.as_str() == "->" { "" } else { "&" };

                    let insert_str = format!(
                        "{{ RC->trackDebugLocDst({}{}, {}, {}, {}, \"{}\", \"{}\"); ",
                        ref_op,
                        debugloc_dst.to_source(code),
                        move_dst,
                        ConstructKind::Moving,
                        call.row(),
                        debugloc_dst.to_source(code),
                        move_dst,
                    );
                    self.add_insert(insert_str, call.start_byte());

                    let insert_str = format!(" }}");
                    self.add_insert(insert_str, call.end_byte() + 1);
                }
                None => {}
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
                    let debugloc_dst_str = debugloc_dst.to_source(code);

                    // We need to distinguish between `Value &` (DLS.replace) and `Value *` (DLS->replace)
                    let field_operator = callee.child(1).unwrap().to_source(code);
                    let prepare_str = format!(
                        "Value *DebugLocSrc = {}{}; Value *DebugLocDst = {};",
                        if field_operator.as_str() == "." {
                            "&"
                        } else {
                            ""
                        },
                        debugloc_src_str,
                        debugloc_dst_str,
                    );

                    let replace_str = format!("DebugLocSrc->replaceAllUsesWith(DebugLocDst);",);

                    let hook_str = format!(
                        "RC->trackDebugLocSrc(DebugLocDst, DebugLocSrc, {}, \"{}\", \"{}\");",
                        call.row(),
                        debugloc_dst_str,
                        debugloc_src_str,
                    );

                    let replace_str = format!("{{ {} {} {} }}", prepare_str, replace_str, hook_str);
                    self.add_replace(replace_str, call.start_byte(), call.end_byte() + 1);
                }
                /* I->replaceUsesOfWith(OldI, NewI); */
                "replaceUsesOfWith" => {
                    if call.parent().unwrap().kind() != "expression_statement" {
                        panic!("{}", "Non expression statement parent!".red().bold());
                    }

                    let called_obj = callee.child_by_field_name("argument").unwrap();
                    let old_inst = arguments.child(1).unwrap();
                    let new_inst = arguments.child(3).unwrap();

                    let field_operator = callee.child(1).unwrap().to_source(code);
                    let prepare_str = format!(
                        "Value *DebugLocSrc = {}; Value *DebugLocDst = {};",
                        old_inst.to_source(code),
                        new_inst.to_source(code),
                    );

                    let inst_repl_str = format!(
                        "{}{}replaceUsesOfWith(DebugLocSrc, DebugLocDst);",
                        called_obj.to_source(code),
                        field_operator,
                    );

                    let hook_str = format!(
                        "RC->trackDebugLocSrc(DebugLocDst, DebugLocSrc, {}, \"{}\", \"{}\");",
                        call.row(),
                        old_inst.to_source(code),
                        new_inst.to_source(code),
                    );

                    let replace_str =
                        format!("{{ {} {} {} }}", prepare_str, inst_repl_str, hook_str);
                    self.add_replace(replace_str, call.start_byte(), call.end_byte() + 1);
                }
                _ => {}
            };

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
                        call.row(),
                        debugloc_dst.to_source(code),
                    );

                    self.add_insert(insert_str, call.end_byte() + 1);
                }
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

                    let insert_str = format!(
                        " RC->trackDebugLocMerging({}, nullptr, nullptr, {}, \"\", \"\", \"\");",
                        debugloc_dst.to_source(code),
                        debugloc_dst.row(),
                    );
                    self.add_insert(insert_str, call.end_byte() + 1);
                }
                Some(DLUpdateKind::Dropping) => {
                    let debugloc_dst = callee.child_by_field_name("argument").unwrap();
                    let addr_op = if callee.child(1).unwrap().to_source(code).as_str() == "->" {
                        ""
                    } else {
                        "&"
                    };

                    let insert_str = format!("{{ ");
                    self.add_insert(insert_str, call.start_byte());

                    let insert_str = format!(
                        " RC->trackDebugLocDropping({}{}, {}, \"{}\"); }}",
                        addr_op,
                        debugloc_dst.to_source(code),
                        call.row(),
                        debugloc_dst.to_source(code),
                    );
                    self.add_insert(insert_str, call.end_byte() + 1);
                }
                None => {}
            };

            self.try_visit_insertions(call, &callee_name, code);
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
                        new.row(),
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
                        new.row(),
                        var_name.to_source(code),
                    );

                    self.add_insert(insert_str, parent_assign.end_byte() + 1);
                    continue;
                }

                if let Some(parent_return) = get_parent_of_kind(&new, "return_statement") {
                    let insert_str = format!(
                        "{{ Value *V = {}; RC->trackDebugLocDst(V, nullptr, {}, {}, \"\", \"\"); ",
                        new.to_source(code),
                        ConstructKind::Creating,
                        new.row(),
                    );
                    self.add_insert(insert_str, parent_return.start_byte());

                    let replace_str = format!("V");
                    self.add_replace(replace_str, new.start_byte(), new.end_byte());

                    let insert_str = format!(" }}");
                    self.add_insert(insert_str, parent_return.end_byte());
                    continue;
                }

                println!(
                    "{}{} {}:\n\t{} {}",
                    "warning".yellow().bold(),
                    ": Encounter an unsupported new expression at line".bold(),
                    new.row(),
                    "->".blue().bold(),
                    new.to_source(code),
                );
            }
        }
    }

    fn visit_fn_defs(&mut self, nodes: Vec<Node>, code: &str) {
        for fn_def in nodes {
            if get_children_of_kind(&fn_def, "function_declarator").len() == 0 {
                println!(
                    "{}{} {}:\n\t{} {}",
                    "warning".yellow().bold(),
                    ": Encounter an function definition without declarator at line".bold(),
                    fn_def.row(),
                    "->".blue().bold(),
                    fn_def.to_source(code)
                );
                continue;
            }
            let fn_ident = get_fn_identifier(&fn_def);
            if fn_ident.to_source(code).is_pass_entry() {
                /* Add initialization and clean up */
                self.collect_init_and_clean_up_edit(&fn_def, code);
            } else {
                /* Process all function calls */
                self.visit_fn_calls(
                    get_children_of_kind(&fn_def, ASTNodeKind::CallExpr.into()),
                    code,
                );
                /* Process all object news */
                self.visit_new_exprs(
                    get_children_of_kind(&fn_def, ASTNodeKind::NewExpr.into()),
                    code,
                );
            }
        }
    }

    fn visit_ast_tree(&mut self, code: &str) {
        let tree = self
            .parser
            .parse(code, None)
            .expect("Failed to parse the code!");
        let root_node = tree.root_node();

        /* Instrument the header include */
        self.visit_header_includes(get_children_of_kind(
            &root_node,
            ASTNodeKind::HeaderInclude.into(),
        ));

        /* Instrument the global varaible */
        self.visit_using_decls(get_children_of_kind(
            &root_node,
            ASTNodeKind::UsingDecl.into(),
        ));

        /* Instrument the hooks */
        self.visit_fn_defs(
            get_children_of_kind(&root_node, ASTNodeKind::FnDef.into()),
            code,
        );
    }
}
