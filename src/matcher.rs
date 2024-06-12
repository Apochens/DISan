use std::fmt::Display;

#[derive(PartialEq)]
pub enum DebugLocUpdateKind {
    Preserving,
    Merging,
    Dropping,
}

impl Display for DebugLocUpdateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            DebugLocUpdateKind::Preserving => "UpdateKind::Preserving",
            DebugLocUpdateKind::Merging => "UpdateKind::Merging",
            DebugLocUpdateKind::Dropping => "UpdateKind::Dropping",
        })
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
        write!(f, "{}", match self {
            ConstructKind::Creating => "ConstructKind::Creating",
            ConstructKind::Moving => "ConstructKind::Moving",
            ConstructKind::Cloning => "ConstructKind::Cloning",
        })
    }
}

pub trait FuncMatch {
    fn is_creation(&self) -> Option<ConstructKind>;
    fn is_replacement(&self) -> bool;
    fn is_debugloc_update(&self) -> Option<DebugLocUpdateKind>;
    fn is_pass_entry(&self) -> bool;
    fn is_insertion(&self) -> bool;
}

const CREATE_FUNC: [&str; 16] = [
    "BinaryOperator::CreateNeg",
    "BinaryOperator::CreateAdd",
    "BinaryOperator::CreateMul",
    "BinaryOperator::CreateSub",

    "BinaryOperator::CreateURem",
    "BinaryOperator::CreateSRem",
    "BinaryOperator::CreateUDiv",
    "BinaryOperator::CreateSDiv",
    "BinaryOperator::CreateLShr",
    "BinaryOperator::CreateAnd",
    "BinaryOperator::Create",

    "CastInst::CreateZExtOrBitCast",
    "CastInst::CreateBitOrPointerCast",

    "PHINode::Create",

    "UnaryOperator::CreateFNegFMF",
    "UnaryOperator::CreateFNeg",
]; 

const CLONE_FUNC: [&str; 1] = [
    "clone"
];

const MOVE_FUNC: [&str; 2] = [
    "moveBefore",
    "moveAfter",
];

const INSERT_FUNC: [&str; 2] = [
    "insertBefore",
    "insertAfter",
];

impl FuncMatch for String {
    fn is_creation(&self) -> Option<ConstructKind> {
        if CREATE_FUNC.contains(&self.as_str()) {
            return Some(ConstructKind::Creating);
        }
        if CLONE_FUNC.contains(&self.as_str()) {
            return Some(ConstructKind::Cloning);
        }
        if MOVE_FUNC.contains(&self.as_str()) {
            return Some(ConstructKind::Moving);
        }
        return None;
    }

    fn is_replacement(&self) -> bool {
        match self.as_str() {
            "replaceAllUsesWith" => true,
            _ => false,
        }
    }

    fn is_debugloc_update(&self) -> Option<DebugLocUpdateKind> {
        match self.as_str() {
            "setDebugLoc" => Some(DebugLocUpdateKind::Preserving),
            "applyMergedLocation" => Some(DebugLocUpdateKind::Merging),
            "dropLocation" => Some(DebugLocUpdateKind::Dropping),
            "updateLocationAfterHoist" => Some(DebugLocUpdateKind::Dropping),
            _ => None,
        }
    }

    fn is_insertion(&self) -> bool {
        if INSERT_FUNC.contains(&self.as_str()) {
            true
        } else {
            false
        }
    }

    fn is_pass_entry(&self) -> bool {
        self.ends_with("Pass::run")
    }
}