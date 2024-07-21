pub struct Hook;

impl Hook {
    pub fn header_include() -> &'static str {
        "#include \"llvm/Transforms/Utils/RuntimeDLChecker.h\"\n"
    }

    pub fn global_var_decl() -> &'static str {
        "namespace { RuntimeChecker *RC = nullptr; }\n"
    }
}