# DISAN

Debug Information Sanitizer (DISan) is the sanitizer-like tool perfoming AST-level instrumentation on LLVM optimizations (Scalar) and providing an in-library support to detect and ***suggest patches*** for debug location update errors (*i.e.*, update rule violations). 

## How to use

1. Copy file `RuntimeCheck.cpp` in this repo's `disan/library` to `/llvm/lib/Transforms/Utils/` under LLVM project. Add `RuntimeCheck.cpp` into the `CMakeList.txt` in the same directory.

2. Change the sanitizing output directory in `RuntimeCheck.h` and then copy it to `/llvm/include/llvm/Transforms/Utils/`.

```cpp
    RuntimeChecker(Function &F, StringRef PN)
        : PassName(PN), 
          ...
    {
->      StringRef DirName = "/path/stub/";
        ...
    }
```

3. Choose a target pass and instrument it using the following command. (Now only passes with single source file in Scalar module are supported) Replace the original pass with the instrumented pass stored in directory `disan/instrumented/`.

```Bash
$ cargo run -- </path/to/target/pass>
```

4. Compile target `opt` in LLVM project.

5. Use `lit` or just `opt` to run the instrumented pass with IR programs. For convenience, one can use the regression tests under the llvm subproject (`/llvm/test/Transforms/`). Once the execution triggers the sanity checks, the sanitizing output will be write to the file with the pass file name in the directory specified by `DirName` in Step 2.