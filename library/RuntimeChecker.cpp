#include "llvm/Transforms/Utils/RuntimeDLChecker.h"
#include "llvm/IR/Instructions.h"
#include <sstream>
#include <iostream>

//===----------------------------------------------------------------------===//
//                          Utils
//===----------------------------------------------------------------------===//

#define RESET      "\033[0m"
#define BOLD       "\033[1m"
#define RED        "\033[31;1m"
#define YELLOW     "\033[93;1m" // 33
#define GREEN      "\033[32;1m"
#define BLUE       "\033[34;1m"

unsigned predNumOf(BasicBlock *BB) {
    return std::distance(pred_begin(BB), pred_end(BB));
}

unsigned succNumOf(BasicBlock *BB) {
    return std::distance(succ_begin(BB), succ_end(BB));
}

StringRef UKindToString(UpdateKind K) {
    switch (K) {
        case UpdateKind::Preserving:
            return "Preserve";
        case UpdateKind::Merging:
            return "Merge";
        case UpdateKind::Dropping:
            return "Drop";
        case UpdateKind::Others:
            return "Any";
        default:
            assert(false && "No such update kind!");
    }
}

StringRef CKindToString(ConstructKind K) {
    switch (K) {
        case ConstructKind::Creating:
            return "Create";
        case ConstructKind::Cloning:
            return "Clone";
        case ConstructKind::Moving:
            return "Move";
        case ConstructKind::Untracked:
            return "Untracked";
        default:
            assert(false && "No such construct kind!");
    }
}

UpdateKind DebugLocDstM::properUpdateKind() {
    switch (CKind) {
        case ConstructKind::Creating: {
            if (ReplacedInstNum == 0) {
                return UpdateKind::Preserving;
            }
        }   // No break here, since the cases below are shared
        case ConstructKind::Moving: {
            if (ReplacedInstNum == 1) {
                if (InReplacedInstDomRegion)
                    return UpdateKind::Preserving;
                else
                    return UpdateKind::Dropping;
            }
            if (ReplacedInstNum >= 2) {
                if (InReplacedInstDomRegion)
                    return UpdateKind::Preserving;
                else
                    return UpdateKind::Merging;
            }
            assert(false);
        }
        case ConstructKind::Cloning: {
            if (ReplacedInstNum == 0) {
                if (InsertPosInOrigDomRegion) 
                    return UpdateKind::Preserving;
                else 
                    return UpdateKind::Dropping;
            }
            if (ReplacedInstNum == 1) {
                if (InsertPosInOrigDomRegion && InReplacedInstDomRegion)
                    return UpdateKind::Preserving;
                else
                    return UpdateKind::Dropping;
            }
            if (ReplacedInstNum >= 2) {
                if (InsertPosInOrigDomRegion && InReplacedInstDomRegion)
                    return UpdateKind::Preserving;
                else
                    return UpdateKind::Merging;
            }
            assert(false);
        }
        case ConstructKind::Untracked: { 
            // Handle untracked instructions involved in instruction replacements.
            dbgs() << YELLOW << "warn: " << "an untracked instruction involved in a replacement!\n" << RESET;
            return UpdateKind::Others;
        }
    }

    assert(false && "No match of proper update kind!");
}

std::string DebugLocDstM::toString() {
    std::stringstream ss;
    UpdateKind ProperKind = properUpdateKind();
    if (InCodeUpdateKind != UpdateKind::None && InCodeUpdateKind == ProperKind) {
        ss << "pass: ";
    } else {
        ss << "fail: "; 
    }

    ss << UKindToString(ProperKind).str();
    ss << " [Construct: " << ConstructSite << ", " << CKindToString(CKind).str();
    if (!ReplaceSite.empty()) {
        ss << "; Replace: ";
        for (auto iter = ReplaceSite.begin(); iter != ReplaceSite.end(); ) {
            ss << *iter;
            if (++iter != ReplaceSite.end())
                ss << ", ";
        }
    }

    if (InCodeUpdateKind != UpdateKind::None)
        ss << "; Update: " << InCodeUpdateSite << ", " << UKindToString(InCodeUpdateKind).str();

    ss << "; Pass: " << VarName;

    ss << "]";
    return ss.str();
}

//===----------------------------------------------------------------------===//
//                          Simple fact queries
//===----------------------------------------------------------------------===//

bool RuntimeChecker::inDominantRegionOf(Instruction *Dst, Instruction *Src) {
    assert(Dst && Src);
    // We don't use isReachableFromEntry to decide whether the given instruction is 
    // inserted into the whole CFG, because the dead code would be considered to be 
    // unreachable. We just check whether their parent functions are the same.
    assert(Dst->getFunction() == Src->getFunction());

    // Renew (Post-)DominatorTree Analysis
    DT->recalculate(*Dst->getFunction());
    PDT->recalculate(*Dst->getFunction());
    
    return Dst->getParent() == Src->getParent() // In the same block
        || DT->dominates(Src, Dst)              // Dominated by Src
        || PDT->dominates(Src, Dst);            // Post-dominated by Src
}

//===----------------------------------------------------------------------===//
//              Track all debug location destinations in the pass
//===----------------------------------------------------------------------===//

void RuntimeChecker::trackDebugLocDstImpl(        
    Instruction *DebugLocDstInst,
    Value *ExtraValue, /* B, A = Create(..., B) or A->moveBefore(B) or A = B->clone() */
    ConstructKind Kind,
    unsigned SrcLine
) {
    assert(DebugLocDstInst);
    InstToDLDMap[DebugLocDstInst] = new DebugLocDstM(PassName.str(), SrcLine, Kind, DebugLocDstInst);

    Instruction *DummyInst = nullptr;
    if (ExtraValue) {
        if (BasicBlock *BB = dyn_cast<BasicBlock>(ExtraValue)) {
            DummyInst = PHINode::Create(DebugLocDstInst->getType(), 0, "", BB);
            dbgs() << "Create dummy: " << *DummyInst << "\n";
            ExtraValue = DummyInst;
        }
    }

    switch (Kind) {
        case ConstructKind::Creating: break;
        case ConstructKind::Cloning: {
            assert(ExtraValue && "The cloned instruction is not given!");
            Instruction *OriginalInst = dyn_cast<Instruction>(ExtraValue);
            assert(OriginalInst && "The cloned instruction is not an instruction!");
            InstToDLDMap[DebugLocDstInst]->setOriginalInst(OriginalInst);
            break;
        }
        case ConstructKind::Moving: {
            assert(ExtraValue && "The destination of the move is not given!");
            Instruction *MovePosInst = dyn_cast<Instruction>(ExtraValue);
            assert(MovePosInst && "The destination of the move is not an instruction!");
            bool IsDominated = inDominantRegionOf(MovePosInst, DebugLocDstInst);
            InstToDLDMap[DebugLocDstInst]->moveAt(SrcLine, IsDominated);
            break;
        }
        default:
            assert(false && "No such construct kind!");
    }

    if (DummyInst) {
        dbgs() << "Destory dummy: " << *DummyInst << '\n';
        DummyInst->removeFromParent();
    }
}

void RuntimeChecker::trackDebugLocDst(
        Value *DebugLocDst,
        Value *ExtraValue,
        ConstructKind Kind,
        unsigned SrcLine,
        std::string DLDName,
        std::string IPName
) {
    if (dyn_cast<BasicBlock>(DebugLocDst)) return ;
    dbgs() << "[TrackDebugLocDst] \033[31;1m" << SrcLine << ":\033[0m " << *DebugLocDst << "\n";

    Instruction *DebugLocDstInst = dyn_cast<Instruction>(DebugLocDst);

    assert(DebugLocDstInst && "Creating a non instruction value!");

    trackDebugLocDstImpl(DebugLocDstInst, ExtraValue, Kind, SrcLine);
}

// void RuntimeChecker::trackDebugLocDst(
//     Value *DebugLocDst,
//     BasicBlock::iterator ExtraIter,
//     ConstructKind Kind,
//     unsigned SrcLine,
//     std::string DLDName,
//     std::string IPName
// ) {
//     dbgs() << "[TrackDebugLocDst] \033[31;1m" << SrcLine << ":\033[0m " << *DebugLocDst << "\n";
//     if (dyn_cast<BasicBlock>(DebugLocDst)) return ;

//     Instruction *DebugLocDstInst = dyn_cast<Instruction>(DebugLocDst);
//     assert(DebugLocDstInst && "Creating a non instruction value!");

//     trackDebugLocDstImpl(DebugLocDstInst, &*ExtraIter, Kind, SrcLine);
// }

//===----------------------------------------------------------------------===//
//              Track all debug location sources in the pass
//===----------------------------------------------------------------------===//

void RuntimeChecker::trackDebugLocSrc(
    Value *DebugLocDst,
    Value *DebugLocSrc, 
    unsigned SrcLine, 
    std::string DLDName, 
    std::string DLSName
) {
    Instruction *DebugLocDstInst = dyn_cast<Instruction>(DebugLocDst);
    Instruction *DebugLocSrcInst = dyn_cast<Instruction>(DebugLocSrc);
    
    if (!DebugLocDstInst || !DebugLocSrcInst)
        return ;

    bool IsDominated = inDominantRegionOf(DebugLocDstInst, DebugLocSrcInst);
    std::string DomStr = IsDominated ? "Dom" : "Not dom";
    
    dbgs() << BLUE << "replace at " << SrcLine << " (" << DomStr << "):" << RESET << "\n\t" 
        << *DebugLocDstInst << " (" << DebugLocDstInst->getParent()->getName() << ")\n\t"
        << *DebugLocSrcInst << " (" << DebugLocSrcInst->getParent()->getName() << ")\n";

    if (InstToDLDMap.contains(DebugLocDstInst)) {
        InstToDLDMap[DebugLocDstInst]->replaceAt(SrcLine, IsDominated);
    } else {
        InstToDLDMap[DebugLocDstInst] = new DebugLocDstM(PassName.str(), SrcLine, ConstructKind::Untracked, DebugLocDstInst);
        InstToDLDMap[DebugLocDstInst]->replaceAt(SrcLine, IsDominated);
    }
}

//===----------------------------------------------------------------------===//
//              Track all debug location updates in the pass
//===----------------------------------------------------------------------===//

void RuntimeChecker::trackDebugLocPreserving(
    Instruction *DebugLocDst,
    Instruction *DebugLocSrc,
    unsigned SrcLine,
    std::string DLDName,
    std::string DLSName
) {
    if (InstToDLDMap.contains(DebugLocDst)) {
        InstToDLDMap[DebugLocDst]->updateAt(SrcLine, UpdateKind::Preserving);
    } else {
        dbgs() << YELLOW << "[TrackPres] Preserving debugloc of an untracked instruction at " << SrcLine << RESET << "\n";
        // assert(false && "Preserving debugloc of an untracked instruction");
    }
}

void RuntimeChecker::trackDebugLocMerging(
    Instruction *DebugLocDst,
    Instruction *DebugLocSrc1,
    Instruction *DebugLocSrc2,
    unsigned SrcLine,
    std::string DLDName,
    std::string DLS1Name,
    std::string DLS2Name
) {
    if (InstToDLDMap.contains(DebugLocDst)) {
        InstToDLDMap[DebugLocDst]->updateAt(SrcLine, UpdateKind::Merging);
    } else {
        dbgs() << YELLOW << "[TrackPres] Merging debugloc of an untracked instruction at " << SrcLine << RESET << "\n";
        // assert(false && "Merging debugloc of an untracked instruction");
    }
}

void RuntimeChecker::trackDebugLocDropping(
    Instruction *DebugLocDst,
    unsigned SrcLine,
    std::string DLDName
) {
    if (InstToDLDMap.contains(DebugLocDst)) {
        InstToDLDMap[DebugLocDst]->updateAt(SrcLine, UpdateKind::Dropping);
    } else {
        dbgs() << YELLOW << "[TrackPres] Dropping debugloc of an untracked instruction at " << SrcLine << RESET << "\n";
        // assert(false && "Dropping debugloc of an untracked instruction");
    }
}

//===----------------------------------------------------------------------===//
//              Track all instruction insertions in the pass
//===----------------------------------------------------------------------===//

void RuntimeChecker::trackInsertionImpl(
    Instruction *InsertInst, 
    Instruction *InsertPosInst,
    unsigned SrcLine
) {
    dbgs() << BLUE << "insertion at " << SrcLine << RESET 
           << "\n\t" << *InsertInst << "\n\t" << *InsertPosInst << "\n";

    if (InstToDLDMap.contains(InsertInst)) {
        if (InstToDLDMap[InsertInst]->constructKind() == ConstructKind::Cloning) {
            // Determine whether the insertion position is dominated by the original 
            // instruction, from which the Inst is cloned from.
            bool IsDominated = inDominantRegionOf(
                InsertPosInst, 
                InstToDLDMap[InsertInst]->originalInst()
            );
            InstToDLDMap[InsertInst]->insertAt(SrcLine, IsDominated);
        }
    }
}

void RuntimeChecker::trackInsertion(
    Value *InsertValue,
    Value *InsertPos,
    unsigned SrcLine,
    std::string DLDName,
    std::string DLSName
) {
    Instruction *InsertInst = dyn_cast<Instruction>(InsertValue);
    Instruction *InsertPosInst = dyn_cast<Instruction>(InsertPos);

    Instruction *DummyInst = nullptr;
    if (!InsertPosInst) {
        if (BasicBlock *BB = dyn_cast<BasicBlock>(InsertPos)) {
            dbgs() << "1\n";
            DummyInst = PHINode::Create(InsertInst->getType(), 0, "", BB);
            InsertPosInst = DummyInst;
                        dbgs() << "2\n";

        }
    }

    if (InsertInst && InsertPosInst)
        trackInsertionImpl(InsertInst, InsertPosInst, SrcLine);

    if (DummyInst)
        DummyInst->removeFromParent();
}

// void RuntimeChecker::trackInsertion(
//     Value *InsertValue,
//     BasicBlock::iterator InsertPos,
//     unsigned SrcLine,
//     std::string DLDName,
//     std::string DLSName
// ) {
//     Instruction *InsertInst = dyn_cast<Instruction>(InsertValue);
//     Instruction *InsertPosInst = &*InsertPos;
//     if (InsertInst && InsertPosInst)
//         trackInsertionImpl(InsertInst, InsertPosInst, SrcLine);
// }

//===----------------------------------------------------------------------===//
//                             Main function
//===----------------------------------------------------------------------===//

void RuntimeChecker::startCheck() {
    for (auto [DebugLocDst, DLDM]: InstToDLDMap) {
        logs() << DLDM->toString() << "\n";
    }
}