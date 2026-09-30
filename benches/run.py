#!/usr/bin/env python3
"""Time each benchmark with Rambo (and Python / Lua when installed).

    cargo build --release
    python3 benches/run.py
"""
import pathlib, shutil, subprocess, sys, time

here = pathlib.Path(__file__).resolve().parent
rambo = here.parent / "target" / "release" / ("rambo.exe" if sys.platform == "win32" else "rambo")
runners = [("Rambo", [str(rambo)], ".rmb")]
if shutil.which("python3") or shutil.which("python"):
    runners.append(("Python", [shutil.which("python3") or shutil.which("python")], ".py"))
lua = shutil.which("lua5.4") or shutil.which("lua")
if lua:
    runners.append(("Lua", [lua], ".lua"))

def best_of(cmd, n=3):
    times, out = [], ""
    for _ in range(n):
        t = time.perf_counter()
        out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
        times.append(time.perf_counter() - t)
    return min(times), out

print(f"{'benchmark':<10}" + "".join(f"{name:>10}" for name, _, _ in runners))
for bench in ["fib", "loop", "closures", "sieve"]:
    row, answers = f"{bench:<10}", set()
    for name, cmd, ext in runners:
        seconds, out = best_of(cmd + [str(here / (bench + ext))])
        answers.add(out)
        row += f"{seconds:>9.2f}s"
    print(row + ("" if len(answers) == 1 else f"   MISMATCH {answers}"))
