from pathlib import Path
import subprocess
from argparse import ArgumentParser, Namespace
import time
import os


CSMITH_BIN_PATH     = "/root/csmith/bin/csmith"
CSMITH_HEADER_PATH  = "/root/csmith/include/"
CLANG_PATH          = "/root/build-trunk/bin/clang"
OPT_PATH            = "/root/build-trunk/bin/opt"

SOURCE_CODE     = "random.c"
IR_PROGRAM      = "random.ll"
EXECUTABLE      = "random"


def get_args() -> Namespace:
    """Argument parsing"""
    parser = ArgumentParser(prog="Fuzzopt")
    parser.add_argument("-t", "--time", type=int, default=60)
    parser.add_argument("-s", "--single", type=str)
    parser.add_argument("-o", "--optlevel", type=int, default=3)
    return parser.parse_args()


def print_progress(elapse: float, total: float, count: int):
    """Progress bar"""
    length = 50
    filled_length = int(length * elapse // total)
    percent = ("{0:.2f}").format(100 * (elapse / total))
    bar = "█" * filled_length + '-' * (length - filled_length)
    print(f"\r Progress: |{bar}| {percent}% (total: {total}s; executed: {count})", end="")


def start_fuzz(time_limit: int, single_pass: str | None, opt_level: int):
    """Main process"""
    start_time = now_time = time.time()
    fuzz_count = 0
    print_progress(now_time - start_time, time_limit, fuzz_count)

    while now_time < start_time + time_limit:
        subprocess.run(f"csmith > {SOURCE_CODE}", capture_output=True, shell=True)

        if single_pass != None:
            print(f"Fuzzing single pass: {single_pass}")
            subprocess.run([CLANG_PATH, SOURCE_CODE, "-Wno-everything", "-I/root/csmith/include", "-S", "-emit-llvm", "-o",  IR_PROGRAM])
            subprocess.run([OPT_PATH, "-S", f"-passes=mem2reg,{single_pass}", IR_PROGRAM, "--disable-output"])
        else:
            subprocess.run([CLANG_PATH, SOURCE_CODE, "-Wno-everything", "-I/root/csmith/include", f"-O{opt_level}", "-o", EXECUTABLE])

        print_progress(now_time - start_time, time_limit, fuzz_count)
        now_time = time.time()

    print_progress(time_limit, time_limit, fuzz_count)
    print()


def cleanup():
    """Clean up"""
    Path(SOURCE_CODE).unlink(missing_ok=True)
    Path(IR_PROGRAM).unlink(missing_ok=True)
    Path(EXECUTABLE).unlink(missing_ok=True)


if __name__ == "__main__":

    args = get_args()

    time_limit: int = args.time
    single_pass: str | None = args.single
    opt_level: int = args.optlevel

    start_fuzz(time_limit, single_pass, opt_level)
    cleanup()
