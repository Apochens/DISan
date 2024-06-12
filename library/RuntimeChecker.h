#ifndef LLVM_TRANSFORM_UTILS_RUNTIME_DEBUGLOC_CHECKER_H
#define LLVM_TRANSFORM_UTILS_RUNTIME_DEBUGLOC_CHECKER_H

#include "llvm/IR/PassManager.h"
#include "llvm/IR/Dominators.h"
#include "llvm/Analysis/PostDominators.h"
#include "llvm/Support/FileSystem.h"

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
    None,
};

class DebugLocDstM {
public:
    DebugLocDstM(std::string VN, unsigned CS, ConstructKind CK)
        : VarName(VN), ConstructSite(CS), CKind(CK), InDomRegion(true),
          ReplacedInstNum(0),
          InCodeUpdateKind(UpdateKind::None) {}
    
    ConstructKind constructKind() const { return CKind; }
    unsigned srcLine() const { return ConstructSite; }
    unsigned replacedInstNum() const { return ReplacedInstNum; }
    Instruction *originalInst() const { return OriginalInst; }
    void setOriginalInst(Instruction *Inst) { OriginalInst = Inst; }

    UpdateKind inCodeUpdateKind() const { return InCodeUpdateKind; }
    UpdateKind properUpdateKind();

    void insertAt(unsigned IS, bool InDR ) {
        InDomRegion = InDomRegion && InDR;
    }
    void replaceAt(unsigned RS, bool InDR) {
        ReplaceSite.insert(RS);
        InDomRegion = InDomRegion && InDR;
        ReplacedInstNum++;
    }

    void updateAt() {

    }
private:
    std::string VarName;

    unsigned ConstructSite;
    ConstructKind CKind;
    Instruction *OriginalInst;

    bool InDomRegion;
    unsigned ReplacedInstNum;
    SmallDenseSet<unsigned, 2> ReplaceSite;

    UpdateKind InCodeUpdateKind;
};

class RuntimeChecker {
public:
    RuntimeChecker(Function &F, FunctionAnalysisManager &FAM, StringRef PN)
        : PassName(PN),
          ModuleName(F.getParent()->getName()),
          FunctionName(F.getName()),
          DT(FAM.getResult<DominatorTreeAnalysis>(F)),
          PDT(FAM.getResult<PostDominatorTreeAnalysis>(F))
    {
        StringRef DirName = "/data16/hshan/tmp/";
        sys::fs::create_directories(DirName);

        std::error_code ErrorCode;
        Twine FileName = DirName + PassName;
        Logs = new raw_fd_ostream(FileName.str(), ErrorCode, sys::fs::OpenFlags::OF_Append);
    }

    void trackDebugLocDst(
        Value *DebugLocDst, 
        Value *InsertPos,
        ConstructKind Kind, 
        unsigned SrcLine, 
        std::string DLDName,
        std::string IPName
    );

    void trackDebugLocDst(
        Value *DebugLocDst,
        BasicBlock::iterator InsertPos,
        ConstructKind Kind,
        unsigned SrcLine,
        std::string DLDName,
        std::string IPName
    );

    void trackDebugLocSrc(
        Value *DebugLocDst,
        Value *DebugLocSrc, 
        unsigned SrcLine, 
        std::string DLDName, 
        std::string DLSName
    );

    void trackDebugLocUpdate(
        Instruction *DebugLocDst,
        Instruction *DebugLocSrc,
        UpdateKind Kind,
        unsigned SrcLine,
        std::string DLDName,
        std::string DLSName
    );

    void trackInsertion(
        Value *DebugLocDst,
        Value *InsertPos,
        unsigned SrcLine,
        std::string DLDName,
        std::string DLSName
    );

    void trackInsertion(
        Value *DebugLocDst,
        BasicBlock::iterator InsertPos,
        unsigned SrcLine,
        std::string DLDName,
        std::string DLSName
    );

    void startCheck();

    ~RuntimeChecker() {
        delete Logs;
        for (auto [I, DLDM]: InstToDLDMap)
            delete DLDM;
    }
private:
    StringRef PassName;
    StringRef ModuleName;
    StringRef FunctionName;
    DominatorTree &DT;
    PostDominatorTree &PDT;

    raw_fd_ostream *Logs;

    DenseMap<Instruction *, DebugLocDstM *> InstToDLDMap;
    DenseMap<unsigned, SmallDenseSet<unsigned, 2>> SrcLineToUpdateMap;

    raw_fd_ostream &logs() { return *Logs; }

    bool inDominantRegionOf(Instruction *DebugLocDst, Instruction *DebugLocSrc);

    void recordUpdate(unsigned SrcLine, UpdateKind Kind);
    void reportUpdate();
};

#endif  // LLVM_TRANSFORM_UTILS_RUNTIME_DEBUGLOC_CHECKER_H