from pathlib import Path
import sys
import re
from typing import Set, Dict

PASS_PATTERN = r"\[Checker\] Pass! (\d+) \((.*)\)"
FAIL_PATTERN = r"\[Checker\] Fail! (\d+) \((.*)\)"

if __name__ == "__main__":
    log_file_path = Path(sys.argv[1])
    if log_file_path.exists():
        content = log_file_path.read_text()

        pass_map = {}
        fail_map: Dict[int, Set[str]] = {}

        for line in content.splitlines():

            if matches := re.match(PASS_PATTERN, line):
                src_line = int(str(matches[1]))
                pass_map[src_line] = str(matches[2])

            if matches := re.match(FAIL_PATTERN, line):
                src_line = int(str(matches[1]))
                if src_line in fail_map:
                    fail_map[src_line].add(str(matches[2]))
                else:
                    fail_map[src_line] = { str(matches[2]) }
                
        for src_line in sorted(pass_map.keys()):
            print(f"\033[32;1mPassed\033[0m: {src_line} ({pass_map[src_line]})")

        for src_line in sorted(fail_map.keys()):
            print(f"\033[31;1mFailed\033[0m: \033[1m{src_line} {fail_map[src_line]}\033[0m")