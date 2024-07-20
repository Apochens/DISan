from pathlib import Path
import sys
import re
from typing import Set, Dict

CONSTRUCT = r"Construct: (\d+)"

def is_pass(s: str) -> bool:
    return s.startswith("pass: ")

def is_fail(s: str) -> bool:
    return s.startswith("fail: ")

def is_warn(s: str) -> bool:
    return s.startswith("warn: ")

def key_construct(s: str) -> int:
    if match := re.search(CONSTRUCT, s):
        return  int(match[1])
    raise RuntimeError

if __name__ == "__main__":
    log_file_path = Path(sys.argv[1])

    if log_file_path.exists():
        content = log_file_path.read_text()
        line_set: Set[str] = set(content.splitlines())
        
        for line in sorted(filter(is_pass, line_set)):
            print(f"[\033[32;1mpass\033[0m]", line.removeprefix("pass: "))

        for line in sorted(filter(is_warn, line_set)):
            print(f"[\033[33;1mwarn\033[0m]", line.removeprefix("warn: "))

        for line in sorted(filter(is_fail, line_set), key=key_construct):
            print(f"[\033[31;1mfail\033[0m]", line.removeprefix("fail: "))