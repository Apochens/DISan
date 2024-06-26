#ifndef LLVM_TRANSFORM_UTILS_RUNTIME_DEBUGLOC_CHECKER_H
#define LLVM_TRANSFORM_UTILS_RUNTIME_DEBUGLOC_CHECKER_H

#include "llvm/IR/PassManager.h"
#include "llvm/IR/Dominators.h"
#include "llvm/Analysis/PostDominators.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Analysis/LoopInfo.h"
#include "llvm/Analysis/LoopAnalysisManager.h"
#include "llvm/Analysis/LoopNestAnalysis.h"

using namespace llvm;

enum class UpdateKind {
    Preserving,
    Merging,
    Dropping,
    Others,
    None,
};

enum class ConstructKind {
    Creating,
    Cloning,
    Moving,
    Untracked,
};

class DebugLocDstM {
public:
    DebugLocDstM(std::string VN, unsigned CS, ConstructKind CK, Instruction *Inst)
        : VarName(VN),
          TheInst(Inst),
          ConstructSite(CS), CKind(CK),

          InsertPosInOrigDomRegion(true),
          InsertSite(0),

          ReplacedInstNum(0),
          InReplacedInstDomRegion(true),

          InCodeUpdateKind(UpdateKind::None),
          InCodeUpdateSite(0)
    {}
    
    ConstructKind constructKind() const { return CKind; }
    void setOriginalInst(Instruction *Inst) { OriginalInst = Inst; }
    Instruction *originalInst() const { return OriginalInst; }
    UpdateKind properUpdateKind();

    void moveAt(unsigned MS, bool InDomRegion) {
        // MoveSite is equal to ConstructSite, so we do not assign the site twice.

        // Moreover, we regard the movement as a replacement, so an instruction
        // constructed by movement replaces at least one instruction.
        InReplacedInstDomRegion = InReplacedInstDomRegion && InDomRegion;
        ReplacedInstNum++;
    }

    void insertAt(unsigned IS, bool InDomRegion) {
        InsertPosInOrigDomRegion = InsertPosInOrigDomRegion && InDomRegion;
        InsertSite = IS;
    }

    void replaceAt(unsigned RS, bool InDR) {
        ReplaceSite.insert(RS);
        InReplacedInstDomRegion = InReplacedInstDomRegion && InDR;
        ReplacedInstNum++;
    }

    void updateAt(unsigned US, UpdateKind UK) {
        InCodeUpdateKind = UK;
        InCodeUpdateSite = US;
    }

    std::string toString();
private:
    std::string VarName;
    Instruction *TheInst;

    /* Construct track (Create, Clone, Move) */
    unsigned ConstructSite;
    ConstructKind CKind;
    Instruction *OriginalInst;

    /* Insert track (for Clone) */
    bool InsertPosInOrigDomRegion;
    unsigned InsertSite;

    /* Replace track */
    unsigned ReplacedInstNum;
    bool InReplacedInstDomRegion;
    SmallDenseSet<unsigned, 2> ReplaceSite;

    /* Update track */
    UpdateKind InCodeUpdateKind;
    unsigned InCodeUpdateSite;
};

class RuntimeChecker {
public:
    RuntimeChecker(Function &F, StringRef PN)
        : PassName(PN), 
          ModuleName(F.getParent()->getName()), 
          FunctionName(F.getName()),
          DT(new DominatorTree(F)),
          PDT(new PostDominatorTree(F))
    {
        StringRef DirName = "/data16/hshan/tmp/";
        sys::fs::create_directories(DirName);

        std::error_code ErrorCode;
        Twine FileName = DirName + PassName;
        Logs = new raw_fd_ostream(FileName.str(), ErrorCode, sys::fs::OpenFlags::OF_Append);
    }

    RuntimeChecker(Loop &L, StringRef PN)
        : RuntimeChecker(*L.getHeader()->getParent(), PN) {}

    RuntimeChecker(LoopNest &LN, StringRef PN)
        : RuntimeChecker(*LN.getParent(), PN) {}

    void trackDebugLocDst(
        Value *DebugLocDst, 
        Value *ExtraValue,
        ConstructKind Kind, 
        unsigned SrcLine, 
        std::string DLDName,    /* Obsoleted */
        std::string IPName      /* Obsoleted */
    );

    void trackDebugLocSrc(
        Value *DebugLocDst,
        Value *DebugLocSrc, 
        unsigned SrcLine, 
        std::string DLDName, 
        std::string DLSName
    );

    void trackDebugLocPreserving(
        Instruction *DebugLocDst,
        Instruction *DebugLocSrc,
        unsigned SrcLine,
        std::string DLDName,
        std::string DLSName
    );

    void trackDebugLocMerging(
        Instruction *DebugLocDst,
        Instruction *DebugLocSrc1,
        Instruction *DebugLocSrc2,
        unsigned SrcLine,
        std::string DLDName,
        std::string DLS1Name,
        std::string DLS2Name
    );

    void trackDebugLocDropping(
        Instruction *DebugLocDst,
        unsigned SrcLine,
        std::string DLDName
    );

    void trackInsertion(
        Value *InsertValue,
        Value *InsertPos,
        unsigned SrcLine,
        std::string DLDName = "",
        std::string DLSName = ""
    );

    void startCheck();

    ~RuntimeChecker() {
        delete Logs;
        for (auto [_, DLDM]: InstToDLDMap) {
            if (DLDM)
                delete DLDM;
        }

        delete DT;
        delete PDT;
    }
private:
    StringRef PassName;
    StringRef ModuleName;
    StringRef FunctionName;
    DominatorTree *DT;
    PostDominatorTree *PDT;
    DenseMap<Instruction *, DebugLocDstM *> InstToDLDMap;

    raw_fd_ostream *Logs;

    raw_fd_ostream &logs() { return *Logs; }

    /* Simple Queries */
    bool inDominantRegionOf(Instruction *DebugLocDst, Instruction *DebugLocSrc);

    /* Main functionality implementations */
    void trackDebugLocDstImpl(
        Instruction *DebugLocDstInst,
        Value *ExtraValue,
        ConstructKind Kind,
        unsigned SrcLine
    );
    void trackDebugLocUpdateImpl();
    void trackInsertionImpl(
        Instruction *Inst,
        Instruction *InsertPosInst,
        unsigned SrcLine
    );
};

#endif  // LLVM_TRANSFORM_UTILS_RUNTIME_DEBUGLOC_CHECKER_H