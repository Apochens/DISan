use std::fmt::Display;

#[derive(PartialEq)]
pub enum DLUpdateKind {
    Preserving,
    Merging,
    Dropping,
}

impl Display for DLUpdateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DLUpdateKind::Preserving => "UpdateKind::Preserving",
                DLUpdateKind::Merging => "UpdateKind::Merging",
                DLUpdateKind::Dropping => "UpdateKind::Dropping",
            }
        )
    }
}

#[derive(PartialEq)]
pub enum ConstructKind {
    Creating,
    Cloning,
    Moving,
}

impl Display for ConstructKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ConstructKind::Creating => "ConstructKind::Creating",
                ConstructKind::Moving => "ConstructKind::Moving",
                ConstructKind::Cloning => "ConstructKind::Cloning",
            }
        )
    }
}

pub trait FuncMatch {
    fn is_construct(&self) -> Option<ConstructKind>;
    fn is_replacement(&self) -> bool;
    fn is_debugloc_update(&self) -> Option<DLUpdateKind>;
    fn is_pass_entry(&self) -> bool;
    fn is_insertion(&self) -> bool;
}

const CREATE_FUNC: [&str; 37] = [
    "BinaryOperator::Create", /* BinaryOperator */
    "BranchInst::Create",     /* BranchInst */
    "CallBase::Create",       /* CallBase */
    "CallBase::addOperandBundle",
    "CallBase::removeOperandBundle",
    "CallBrInst::Create",         /* CallBrInst */
    "CallInst::Create",           /* CallInst */
    "CmpInst::Create",            /* CmpInst */
    "FCmpInst",                   /* FCmpInst */
    "ICmpInst",                   /* ICmpInst */
    "ExtractElementInst::Create", /* ExtractElementInst */
    "GetElementPtrInst::Create",  /* GetElementPtrInst */
    "InsertElementInst::Create",  /* InsertElementInst */
    "InsertValueInst::Create",    /* InsertValueInst */
    "PHINode::Create",            /* PHINode */
    "ReturnInst::Create",         /* ReturnInst */
    "SelectInst::Create",         /* SelectInst */
    "StoreInst",                  /* StoreInst */
    "SwitchInst::Create",         /* SwitchInst */
    "UnaryOperator::Create",
    "LoadInst",
    "FreezeInst",
    "ExtractValueInst::Create",
    "CastInst::Create", /* CastInst */
    "AddrSpaceCastInst",
    "BitCastInst",
    "FPExtInst",
    "FPToSIInst",
    "FPToUIInst",
    "FPTruncInst",
    "IntToPtrInst",
    "PtrToIntInst",
    "SExtInst",
    "SIToFPInst",
    "TruncInst",
    "UIToFPInst",
    "ZExtInst",
];

const CLONE_FUNC: [&str; 1] = ["clone"];

const MOVE_FUNC: [&str; 3] = ["moveBefore", "moveBeforePreserving", "moveAfter"];

const INSERT_FUNC: [&str; 3] = ["insertBefore", "insertAfter", "insertInto"];

impl FuncMatch for String {
    fn is_construct(&self) -> Option<ConstructKind> {
        for prefix in CREATE_FUNC {
            if prefix.contains("::") {
                if self.starts_with(prefix) {
                    return Some(ConstructKind::Creating);
                }
            } else {
                if self.as_str() == prefix {
                    return Some(ConstructKind::Creating);
                }
            }
        }
        if CLONE_FUNC.contains(&self.as_str()) {
            return Some(ConstructKind::Cloning);
        }
        if MOVE_FUNC.contains(&self.as_str()) {
            return Some(ConstructKind::Moving);
        }

        None
    }

    fn is_replacement(&self) -> bool {
        match self.as_str() {
            "replaceAllUsesWith" => true,
            "replaceUsesOfWith" => true,
            _ => false,
        }
    }

    fn is_debugloc_update(&self) -> Option<DLUpdateKind> {
        match self.as_str() {
            "setDebugLoc" => Some(DLUpdateKind::Preserving),
            "applyMergedLocation" => Some(DLUpdateKind::Merging),
            "dropLocation" => Some(DLUpdateKind::Dropping),
            "updateLocationAfterHoist" => Some(DLUpdateKind::Dropping),
            _ => None,
        }
    }

    fn is_insertion(&self) -> bool {
        INSERT_FUNC.contains(&self.as_str())
    }

    fn is_pass_entry(&self) -> bool {
        self.ends_with("Pass::run")
    }
}
