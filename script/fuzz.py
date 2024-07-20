import subprocess
from argparse import ArgumentParser
import time


CSMITH_BIN_PATH = "/root/csmith/bin/csmith"
CSMITH_HEADER_PATH = "/root/csmith/include/"
CLANG_PATH = "/root/build-trunk/bin/clang"
OPT_PATH = "/root/build-trunk/bin/opt"

FUZZ_TARGET = "correlated-propagation"


def print_progress(elapse: float, total: float, count: int):
    length = 50
    filled_length = int(length * elapse // total)
    percent = ("{0:.2f}").format(100 * (elapse / total))
    bar = "█" * filled_length + '-' * (length - filled_length)
    print(f"\r Progress: |{bar}| {percent}% (total: {total}s; executed: {count})", end="")


if __name__ == "__main__":

    parser = ArgumentParser(prog="Fuzzopt")
    parser.add_argument("-t", "--time", type=int, default=60)
    parser.add_argument("-s", "--single", type=str)
    parser.add_argument("-o", "--optlevel", type=int, default=3)
    args = parser.parse_args()

    time_limit: int = args.time
    single_pass: str | None = args.single
    opt_level: int = args.optlevel

    start_time = time.time()
    now_time = time.time()
    fuzz_count = 0
    print_progress(now_time - start_time, time_limit, fuzz_count)

    while now_time < start_time + time_limit:
        source_file = "random.c"
        ir_file = "random.ll"
        bin_file = "random"
        subprocess.run(f"csmith > {source_file}", capture_output=True, shell=True)

        if single_pass != None:
            print(f"Fuzzing single pass: {single_pass}")
            subprocess.run([CLANG_PATH, source_file, "-Wno-everything", "-I/root/csmith/include", "-S", "-emit-llvm", "-o",  ir_file])
            subprocess.run([OPT_PATH, "-S", f"-passes=mem2reg,{single_pass}", ir_file, "--disable-output"])
        else:
            subprocess.run([CLANG_PATH, source_file, "-Wno-everything", "-I/root/csmith/include", f"-O{opt_level}", "-o", bin_file])

        print_progress(now_time - start_time, time_limit, fuzz_count)
        now_time = time.time()

    print_progress(time_limit, time_limit, fuzz_count)
    print()

