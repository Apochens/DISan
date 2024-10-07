import sys
from pathlib import Path


if __name__ == "__main__":
    llvm_root = Path(sys.argv[1]).resolve()
    if not llvm_root.exists() or not llvm_root.is_dir():
        print(f"Please give the absolute/relative path to llvm project! No such path: {llvm_root}")
        exit(255)

    include_path = llvm_root.joinpath("llvm").joinpath("include").joinpath("llvm").joinpath("Transforms").joinpath("Utils")
    if not include_path.exists() or not include_path.is_dir():
        print(f"Cannot find the include directory: {include_path}")
        exit(255)

    src_path = llvm_root.joinpath("llvm").joinpath("lib").joinpath("Transforms").joinpath("Utils")
    if not src_path.exists() or not src_path.is_dir():
        print(f"Cannot find the source directory: {src_path}")
        exit(255)
    
    cmakelist_path = src_path.joinpath("CmakeLists.txt")
    if not cmakelist_path.exists() or not cmakelist_path.is_file():
        print(f"Could not find the CmakeLists.txt: {cmakelist_path}")
        exit(255)

    

    
    
