# Rewrite `global_get(<expr of type Str>)` to `str_get(...)` at the spans rustc reports (T100).
import json, subprocess, sys, collections
root = sys.argv[1]
out = subprocess.run(["cargo", "build", "-p", "js2wasi3-backend", "--message-format=json"],
                     cwd=root, capture_output=True, text=True).stdout
edits = collections.defaultdict(set)
left = 0
for line in out.splitlines():
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") != "compiler-message":
        continue
    msg = m["message"]
    if msg.get("level") != "error":
        continue
    done = False
    if msg.get("code") and msg["code"]["code"] == "E0308":
        for sp in msg["spans"]:
            if not sp["is_primary"]:
                continue
            lab = sp.get("label") or ""
            if "expected `u32`, found `Str`" in lab or "found `Str`" in lab:
                edits[sp["file_name"]].add((sp["line_start"], sp["column_start"]))
                done = True
    if not done:
        left += 1
fixed = 0
for f, spots in edits.items():
    path = f"{root}/{f}"
    lines = open(path).read().split("\n")
    for (ln, col) in sorted(spots, reverse=True):
        l = lines[ln - 1]
        pre = l[: col - 1]
        if pre.endswith("global_get("):
            lines[ln - 1] = pre[: -len("global_get(")] + "str_get(" + l[col - 1 :]
            fixed += 1
        else:
            print("other", f, ln, l.strip())
    open(path, "w").write("\n".join(lines))
print("fixed", fixed, "other errors", left)
