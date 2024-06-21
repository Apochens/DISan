#include "llvm/Transforms/Utils/RuntimeDLChecker.h"
#include "llvm/IR/Instructions.h"

void dbg(StringRef Tag, StringRef Content) {
    dbgs() << "[" << Tag << "] " << Content << "\n";
}

void fail(std::string Tag, std::string Content) {
    std::string f = "\033[31mFail\033[0m: " + Content;
    dbg(Tag, f);
}

void pass(std::string Tag, std::string Content) {
    std::string p = "\033[32mPass\033[0m: " + Content;
    dbg(Tag, p);
}

StringRef ToString(UpdateKind K) {
    switch (K) {
    case UpdateKind::Preserving:
        return "Preserve";
    case UpdateKind::Merging:
        return "Merge";
    case UpdateKind::Dropping:
        return "Drop";
    default:
        assert(false && "No such update kind!");
    }
}

UpdateKind DebugLocDstM::properUpdateKind() {
    if (CKind == ConstructKind::None)
        // Only has replacement, but has no construction.
        return UpdateKind::Others;

    if (ReplacedInstNum == 0) {
        // When no instruction is replaced by the new instruction
        if (!InDomRegion)   // Moving or Cloning
            return UpdateKind::Dropping;
        return UpdateKind::Preserving;
    }

    if (ReplacedInstNum == 1) {
        if (InDomRegion)
            return UpdateKind::Preserving;
        else
            return UpdateKind::Dropping;
    }

    if (ReplacedInstNum >= 2) {
        if (InDomRegion)
            return UpdateKind::Preserving;
        else
            return UpdateKind::Merging;
    }

    return UpdateKind::Others;
}


bool RuntimeChecker::inDominantRegionOf(Instruction *DebugLocDst, Instruction *DebugLocSrc) {
    // Renew (Post-)DominatorTree Analysis
    DT->recalculate(*DebugLocDst->getFunction());
    PDT->recalculate(*DebugLocDst->getFunction());

    return DebugLocDst->getParent() == DebugLocSrc->getParent() 
        || DT->dominates(DebugLocSrc, DebugLocDst) 
        || PDT->dominates(DebugLocSrc, DebugLocDst);
}

void RuntimeChecker::recordUpdate(Instruction *DebugLocDstInst, UpdateKind Kind) {
    unsigned KindID = static_cast<unsigned>(Kind);
    DebugLocDstM *DLDM = InstToDLDMap[DebugLocDstInst];

    unsigned SrcLine = DLDM->srcLine();

    if (SrcLineToUpdateMap.contains(SrcLine)) {
        SrcLineToUpdateMap[SrcLine].insert(KindID);
    } else {
        SrcLineToUpdateMap[SrcLine] = { KindID };
    }
}

void RuntimeChecker::reportUpdate() {
    for (auto [SrcLine, UpdateSet]: SrcLineToUpdateMap) {
        logs() << "[Checker] Fail! " << SrcLine << " (";
        for (unsigned Kind: UpdateSet) {
            logs() << ToString(static_cast<UpdateKind>(Kind)) << " ";
        }
        logs() << ")\n";
    }
}

void RuntimeChecker::trackDebugLocDst(
        Value *DebugLocDst,
        Value *InsertPos,
        ConstructKind Kind,
        unsigned SrcLine,
        std::string DLDName,
        std::string IPName
) {
    dbgs() << "[TrackDebugLocDst] " << *DebugLocDst << "\n";

    Instruction *DebugLocDstInst = dyn_cast<Instruction>(DebugLocDst);
    assert(DebugLocDstInst && "[TrackDebugLocDst] Creating a non instruction value!");

    InstToDLDMap[DebugLocDstInst] = new DebugLocDstM(DLDName, SrcLine, Kind);
    switch (Kind) {
        case ConstructKind::Creating: break;
        case ConstructKind::Cloning: {
            Instruction *OriginalInst = dyn_cast<Instruction>(InsertPos);
            assert(OriginalInst && "[TrackDebugLocDst] The instruction that is cloned is not given!");
            InstToDLDMap[DebugLocDstInst]->setOriginalInst(OriginalInst);
            break;
        }
        case ConstructKind::Moving: {
            Instruction *MovePosInst = dyn_cast<Instruction>(InsertPos);
            assert(MovePosInst && "[TrackDebugLocDst] The destination of the move is not an instruction!");
            bool IsDominated = inDominantRegionOf(MovePosInst, DebugLocDstInst);
            InstToDLDMap[DebugLocDstInst]->insertAt(SrcLine, IsDominated);
            break;
        }
        default:
            assert(false && "[TrackDebugLocDst] No such construct kind!");
    }
}

void RuntimeChecker::trackDebugLocDst(
    Value *DebugLocDst,
    BasicBlock::iterator InsertPos,
    ConstructKind Kind,
    unsigned SrcLine,
    std::string DLDName,
    std::string IPName
) {
    dbgs() << "[TrackDebugLocDst] " << *DebugLocDst << "\n";

    Instruction *DebugLocDstInst = dyn_cast<Instruction>(DebugLocDst);
    assert(DebugLocDstInst && "[TrackDebugLocDst] Creating a non instruction value!");

    InstToDLDMap[DebugLocDstInst] = new DebugLocDstM(DLDName, SrcLine, Kind);
    switch (Kind) {
        case ConstructKind::Creating: break;
        case ConstructKind::Cloning: break;
        case ConstructKind::Moving: {
            Instruction *MovePosInst = dyn_cast<Instruction>(&*InsertPos);
            assert(MovePosInst && "[TrackDebugLocDst] The destination of the move is not an instruction!");
            bool IsDominated = inDominantRegionOf(MovePosInst, DebugLocDstInst);
            InstToDLDMap[DebugLocDstInst]->insertAt(SrcLine, IsDominated);
            break;
        }
        default:
            assert(false && "[TrackDebugLocDst] No such construct kind!");
    }
}

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
    // bool IsReachable = isReachableFromEntry(DebugLocDstInst->getParent());
    
    dbgs() << "[TrackDebugLocSrc] " << IsDominated << "\n\t" 
        << *DebugLocDstInst << " (" << DebugLocDstInst->getParent()->getName() << ")\n\t"
        << *DebugLocSrcInst << " (" << DebugLocSrcInst->getParent()->getName() << ")\n";

    if (DebugLocDstM *DLDM = InstToDLDMap[DebugLocDstInst]) {
        DLDM->replaceAt(SrcLine, IsDominated);
    } else {
        InstToDLDMap[DebugLocDstInst] = new DebugLocDstM(DLDName, SrcLine, ConstructKind::None);
        InstToDLDMap[DebugLocDstInst]->replaceAt(SrcLine, IsDominated);
    }
}

void RuntimeChecker::trackDebugLocPreserving(
    Instruction *DebugLocDst,
    Instruction *DebugLocSrc,
    unsigned SrcLine,
    std::string DLDName,
    std::string DLSName
) {
    if (DebugLocDstM *DLDM = InstToDLDMap[DebugLocDst]) {
        DLDM->setInCodeUpdateKind(UpdateKind::Preserving);
    } else {
        assert(false && "Preserving debugloc of an untracked instruction");
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
    if (DebugLocDstM *DLDM = InstToDLDMap[DebugLocDst]) {
        DLDM->setInCodeUpdateKind(UpdateKind::Merging);
    } else {
        assert(false && "Merging debugloc of an untracked instruction");
    }
}

void RuntimeChecker::trackDebugLocDropping(
    Instruction *DebugLocDst,
    unsigned SrcLine,
    std::string DLDName
) {
    if (DebugLocDstM *DLDM = InstToDLDMap[DebugLocDst]) {
        DLDM->setInCodeUpdateKind(UpdateKind::Dropping);
    } else {
        assert(false && "Dropping debugloc of an untracked instruction");
    }
}

void RuntimeChecker::trackInsertion(
    Value *DebugLocDst,
    Value *InsertPos,
    unsigned SrcLine,
    std::string DLDName,
    std::string DLSName
) {
    Instruction *DebugLocDstInst = dyn_cast<Instruction>(DebugLocDst);
    Instruction *InsertPosInst = dyn_cast<Instruction>(InsertPos);
    if (!DebugLocDstInst || !InsertPosInst) return ;

    dbgs() << "[TrackInsertion]" << *DebugLocDstInst << "\t" << *InsertPosInst << "\n";

    if (DebugLocDstM *DLDM = InstToDLDMap[DebugLocDstInst]) {
        if (DLDM->constructKind() == ConstructKind::Cloning) {
            bool IsDominated = inDominantRegionOf(DLDM->originalInst(), InsertPosInst);
            DLDM->insertAt(SrcLine, IsDominated);
        }
    }
}

void RuntimeChecker::trackInsertion(
    Value *DebugLocDst,
    BasicBlock::iterator InsertPos,
    unsigned SrcLine,
    std::string DLDName,
    std::string DLSName
) {
    Instruction *DebugLocDstInst = dyn_cast<Instruction>(DebugLocDst);
    if (!DebugLocDstInst) return ;

    dbgs() << "[TrackInsertion]" << *DebugLocDstInst << "\t" << *InsertPos << "\n";

    if (DebugLocDstM *DLDM = InstToDLDMap[DebugLocDstInst]) {
        if (DLDM->constructKind() == ConstructKind::Cloning) {
            bool IsDominated = inDominantRegionOf(DLDM->originalInst(), &*InsertPos);
            DLDM->insertAt(SrcLine, IsDominated);
        }
    }
}

void RuntimeChecker::startCheck() {
    if (!InstToDLDMap.empty()) {
        dbgs() << "[Checker] Start checking @" << FunctionName << " (" << ModuleName << ")" << ":\n";
        logs() << "[Checker] Start checking @" << FunctionName << " (" << ModuleName << ")" << ":\n";
    }

    for (auto [DebugLocDst, DLDM]: InstToDLDMap) {
        UpdateKind ProperKind = DLDM->properUpdateKind();
        UpdateKind InCodeKind = DLDM->inCodeUpdateKind();

        if (InCodeKind != UpdateKind::None && InCodeKind == ProperKind) {
            dbgs() << "[Checker] \033[32mPass!\033[0m" << " (\033[32m" << ToString(ProperKind) << "\033[0m)" << "\n";
            logs() << "[Checker] Pass! " << DLDM->srcLine() << " (" << ToString(ProperKind) << ")\n";
        } else {
            switch (ProperKind) {
                case UpdateKind::Preserving: {
                    fail("Checker", "should preserve!");
                    recordUpdate(DebugLocDst, ProperKind);
                    break;
                }
                case UpdateKind::Merging: {
                    fail("Checker", "should merge!");
                    recordUpdate(DebugLocDst, ProperKind);
                    break;
                }
                case UpdateKind::Dropping: {
                    fail("Checker", "should drop!");
                    recordUpdate(DebugLocDst, ProperKind);
                    break;
                }
                default:
                    dbgs() << "[Checker] Unknown: could preserve or drop.\n";
                    break;
            }
        }
    }

    reportUpdate();

    if (!InstToDLDMap.empty()) {
        dbgs() << "[Checker] Finish checking.\n\n";
        logs() << "[Checker] Finish checking.\n\n";
    }
}