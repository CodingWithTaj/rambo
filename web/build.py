#!/usr/bin/env python3
"""Build the Rambo website into web/dist/.

    cargo build --lib --release --target wasm32-unknown-unknown
    python web/build.py

Produces:
    dist/index.html                  landing page
    dist/playground/index.html       the playground
    dist/docs/*.html                 documentation, from docs/*.md
    dist/rambo.wasm                  the interpreter, shared by every page
    dist/assets/                     shared CSS and JavaScript
    dist/rambo-playground.html       a single self-contained playground file

No dependencies beyond the Python standard library (including a small
Markdown converter below that handles exactly what the docs use).
"""
import base64, html, json, pathlib, re, shutil, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
WEB = ROOT / "web"
DIST = WEB / "dist"
DEFAULT_WASM = ROOT / "target" / "wasm32-unknown-unknown" / "release" / "rambo.wasm"

def read(p): return pathlib.Path(p).read_text(encoding="utf-8")
def write(p, text):
    p = pathlib.Path(p)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")

# Documentation pages, in reading order: (file stem, title, section)
PAGES = [
    ("getting-started", "Getting started", "Guide"),
    ("basics", "Basics", "Guide"),
    ("control-flow", "Control flow", "Guide"),
    ("functions", "Functions", "Guide"),
    ("collections", "Collections", "Guide"),
    ("errors", "Errors & debugging", "Guide"),
    ("stdlib", "Standard library", "Reference"),
    ("grammar", "Grammar", "Reference"),
    ("internals", "How it works", "Under the hood"),
    ("performance", "Performance", "Under the hood"),
]
EXAMPLE_ORDER = ["hello", "closures", "maps", "functional", "errors", "garbage", "sorting", "primes", "fib"]
FONTS = ('<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>\n'
         '<link href="https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,400..800&family=IBM+Plex+Mono:wght@400;500;600&display=swap" rel="stylesheet">')

# ------------------------------------------------------------------ Markdown

def slug(text):
    text = re.sub(r"<[^>]+>", "", text).lower()
    text = re.sub(r"[^\w\s-]", "", text)
    return re.sub(r"\s+", "-", text.strip())

def inline(text):
    """Inline Markdown: `code`, **bold**, *italic*, [links](url)."""
    codes = []
    def keep_code(m):
        codes.append("<code>" + html.escape(m.group(1), quote=False) + "</code>")
        return f"\x00{len(codes) - 1}\x00"
    text = re.sub(r"`([^`]+)`", keep_code, text)
    text = text.replace("\\|", "|")
    text = html.escape(text, quote=False)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", text)
    def link(m):
        label, url = m.group(1), m.group(2)
        url = re.sub(r"^([\w-]+)\.md(#|$)", r"\1.html\2", url)
        ext = ' rel="noopener"' if url.startswith("http") else ""
        return f'<a href="{html.escape(url)}"{ext}>{label}</a>'
    text = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, text)
    return re.sub(r"\x00(\d+)\x00", lambda m: codes[int(m.group(1))], text)

def split_row(line):
    cells = re.split(r"(?<!\\)\|", line.strip().strip("|"))
    return [c.strip() for c in cells]

def markdown(md):
    lines = md.replace("\r\n", "\n").split("\n")
    out, i, title = [], 0, ""
    para = []
    def flush():
        if para:
            out.append("<p>" + inline(" ".join(para)) + "</p>")
            para.clear()
    while i < len(lines):
        line = lines[i]
        if line.startswith("```"):
            flush()
            lang = line[3:].strip()
            body = []
            i += 1
            while i < len(lines) and not lines[i].startswith("```"):
                body.append(lines[i]); i += 1
            i += 1
            code = html.escape("\n".join(body), quote=False)
            if lang == "rambo":
                out.append('<div class="runnable"><pre><code>' + code + '</code></pre>'
                           '<div class="tools"><a class="btn open" href="#">Edit</a><button class="btn primary run">Run</button></div>'
                           '<pre class="out" aria-live="polite"></pre></div>')
            else:
                out.append(f'<pre class="plain"><code>{code}</code></pre>')
            continue
        m = re.match(r"^(#{1,3})\s+(.*)", line)
        if m:
            flush()
            level, text = len(m.group(1)), m.group(2)
            if level == 1:
                title = text
                out.append(f"<h1>{inline(text)}</h1>")
            else:
                sid = slug(text)
                out.append(f'<h{level} id="{sid}">{inline(text)}<a class="anchor" href="#{sid}" aria-label="Link to this section">#</a></h{level}>')
            i += 1; continue
        if line.startswith("|"):
            flush()
            rows = []
            while i < len(lines) and lines[i].startswith("|"):
                rows.append(lines[i]); i += 1
            head, body = split_row(rows[0]), [split_row(r) for r in rows[2:]]
            t = "<table><thead><tr>" + "".join(f"<th>{inline(c)}</th>" for c in head) + "</tr></thead><tbody>"
            for r in body:
                t += "<tr>" + "".join(f"<td>{inline(c)}</td>" for c in r) + "</tr>"
            out.append(t + "</tbody></table>")
            continue
        m = re.match(r"^(\s*)([-*]|\d+\.)\s+(.*)", line)
        if m:
            flush()
            ordered = m.group(2)[0].isdigit()
            items = []
            while i < len(lines):
                m = re.match(r"^(\s*)([-*]|\d+\.)\s+(.*)", lines[i])
                if m:
                    items.append(m.group(3))
                elif lines[i].startswith("  ") and lines[i].strip() and items:
                    items[-1] += " " + lines[i].strip()
                else:
                    break
                i += 1
            tag = "ol" if ordered else "ul"
            out.append(f"<{tag}>" + "".join(f"<li>{inline(x)}</li>" for x in items) + f"</{tag}>")
            continue
        if line.startswith(">"):
            flush()
            quote = []
            while i < len(lines) and lines[i].startswith(">"):
                quote.append(lines[i][1:].strip()); i += 1
            out.append("<blockquote><p>" + inline(" ".join(quote)) + "</p></blockquote>")
            continue
        if not line.strip():
            flush(); i += 1; continue
        para.append(line.strip()); i += 1
    flush()
    return title, "\n".join(out)

# ------------------------------------------------------------------ pages

def page(title, body, depth, head_extra="", scripts=""):
    up = "../" * depth
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>{html.escape(title)}</title>
<link rel="icon" href="data:image/svg+xml,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 100 100%22><text y=%22.9em%22 font-size=%2290%22>🐕</text></svg>">
{FONTS}
<link rel="stylesheet" href="{up}assets/site.css">
{head_extra}
</head>
<body>
{body}
<script>window.RAMBO_WASM = "{up}rambo.wasm";</script>
<script src="{up}assets/rambo.js"></script>
{scripts}
</body>
</html>
"""

def bar(depth, current=""):
    up = "../" * depth
    def item(href, label, key):
        cur = ' aria-current="page"' if key == current else ""
        return f'<a href="{up}{href}"{cur}>{label}</a>'
    return f"""<header class="bar"><div class="bar-inner">
  <a class="mark" href="{up}index.html">rambo</a>
  <nav>{item("docs/getting-started.html", "Docs", "docs")}{item("playground/index.html", "Playground", "play")}<a href="https://github.com/CodingWithTaj/rambo">GitHub</a></nav>
</div></header>"""

def build_docs():
    sections = {}
    for stem, title, section in PAGES:
        sections.setdefault(section, []).append((stem, title))
    for n, (stem, title, _) in enumerate(PAGES):
        _, content = markdown(read(ROOT / "docs" / f"{stem}.md"))
        nav = ""
        for section, items in sections.items():
            nav += f"<h3>{section}</h3>"
            for s, t in items:
                cur = ' aria-current="page"' if s == stem else ""
                nav += f'<a href="{s}.html"{cur}>{html.escape(t)}</a>'
        pager = '<nav class="pager" aria-label="Pages">'
        if n > 0:
            ps, pt, _ = PAGES[n - 1]
            pager += f'<a href="{ps}.html"><span>Previous</span>{html.escape(pt)}</a>'
        if n + 1 < len(PAGES):
            ns, nt, _ = PAGES[n + 1]
            pager += f'<a class="next" href="{ns}.html"><span>Next</span>{html.escape(nt)}</a>'
        pager += "</nav>"
        body = (bar(1, "docs") +
                '<div class="docs"><aside class="side" id="side" aria-label="Documentation">' + nav + "</aside>"
                '<main class="prose"><button class="btn menu-toggle" onclick="document.getElementById(\'side\').classList.toggle(\'open\')">Contents</button>'
                + content + pager + "</main></div>")
        write(DIST / "docs" / f"{stem}.html",
              page(f"{title} | Rambo", body, 1, scripts='<script>Rambo.wireRunnables("../playground/index.html");</script>'))
    write(DIST / "docs" / "index.html", '<!doctype html><meta http-equiv="refresh" content="0; url=getting-started.html">')

LANDING = """
<main class="land">
  <section class="hero">
    <div class="hero-text">
      <h1 class="word">rambo</h1>
      <p class="lede">A small, fast scripting language with closures, maps, a garbage collector and a step-through debugger. Written in Rust, and running right here in your browser through WebAssembly.</p>
      <p class="actions"><a class="btn primary" href="docs/getting-started.html">Read the guide</a><a class="btn" href="playground/index.html">Open the playground</a></p>
    </div>
    <div class="hero-code">
      <div class="runnable"><pre><code>{hero}</code></pre><div class="tools"><a class="btn open" href="#">Edit</a><button class="btn primary run">Run</button></div><pre class="out" aria-live="polite"></pre></div>
    </div>
  </section>

  <section class="points">
    <div>
      <h2>Familiar on sight</h2>
      <p>Curly braces, <code>let</code>, first-class functions and closures, lists and maps with dot access, and string interpolation. If you've written JavaScript, Python or Lua, you can read Rambo already.</p>
      <p><a href="docs/basics.html">Learn the basics</a></p>
    </div>
    <div>
      <h2>Errors that point</h2>
<pre class="plain errbox"><code>runtime error: can't add number and nil
  --> line 3, column 22
  |
3 |     return total(xs) + nil;
  |                      ^
  = at average (line 3)
    at &lt;script&gt; (line 7)</code></pre>
      <p>Every error names the problem, marks the exact column, and shows the call stack that led there.</p>
    </div>
    <div>
      <h2>See inside</h2>
      <p>The <a href="playground/index.html">playground</a> shows the bytecode your program compiles to, and its debugger steps line by line with breakpoints, local variables and the VM's value stack in view.</p>
      <p><a href="docs/internals.html">How it works</a></p>
    </div>
  </section>

  <section class="numbers">
    <h2>Quick enough to be useful</h2>
    <p>A bytecode compiler, a mark-and-sweep garbage collector and interned strings put Rambo within 1.4–2× of CPython on typical benchmarks, and about 4× faster than its first version. <a href="docs/performance.html">How the speed-ups were found</a>.</p>
    <table>
      <thead><tr><th>Benchmark</th><th>Rambo</th><th>Python 3.12</th><th>Lua 5.4</th></tr></thead>
      <tbody>
        <tr><td>fib(30)</td><td>0.11 s</td><td>0.08 s</td><td>0.05 s</td></tr>
        <tr><td>10M-iteration loop</td><td>0.48 s</td><td>0.32 s</td><td>0.10 s</td></tr>
        <tr><td>2M closure calls</td><td>0.11 s</td><td>0.08 s</td><td>0.04 s</td></tr>
        <tr><td>Sieve of 2M</td><td>0.40 s</td><td>0.20 s</td><td>0.10 s</td></tr>
      </tbody>
    </table>
  </section>
</main>
<footer class="foot"><p>Rambo is named after a very good dog. Released under the MIT license.</p></footer>
"""

LANDING_CSS = """<style>
.land { max-width: 1240px; margin: 0 auto; padding: 0 24px; }
.hero { display: grid; grid-template-columns: minmax(0, 0.95fr) minmax(0, 1.05fr); gap: 56px; align-items: center; padding: 64px 0 72px; }
.hero-text { min-width: 0; }
.word { font-size: clamp(76px, 9.5vw, 150px); font-weight: 800; letter-spacing: -0.065em; line-height: 0.8; margin: 0 0 28px; font-variation-settings: "opsz" 96; }
.word::after { content: "."; color: var(--tan); }
.lede { font-size: 21px; line-height: 1.5; max-width: 34ch; margin: 0 0 28px; }
.actions { display: flex; gap: 10px; flex-wrap: wrap; }
.actions .btn { padding: 12px 18px; font-size: 15px; text-decoration: none; }
.hero-code .runnable { margin: 0; box-shadow: 0 1px 0 var(--line), 0 18px 40px -24px rgba(23,32,43,.35); }
.hero-code pre { font-size: 14.5px; }
.points { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 40px; padding: 48px 0; border-top: 1px solid var(--line); }
.points h2, .numbers h2 { font-size: 24px; letter-spacing: -0.02em; margin: 0 0 10px; }
.points p { margin: 0 0 12px; color: var(--muted); }
.points p code { color: var(--ink); background: var(--sunk); padding: 1px 5px; border-radius: 4px; }
.errbox { font-size: 12.5px; margin: 0 0 12px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r); padding: 12px 14px; overflow-x: auto; }
.numbers { padding: 48px 0 72px; border-top: 1px solid var(--line); max-width: 760px; }
.numbers > p { color: var(--muted); max-width: 62ch; }
.numbers table { border-collapse: collapse; width: 100%; font-size: 15px; margin-top: 20px; }
.numbers th { text-align: left; border-bottom: 2px solid var(--ink); padding: 6px 12px 6px 0; }
.numbers td { border-bottom: 1px solid var(--line); padding: 8px 12px 8px 0; font-variant-numeric: tabular-nums; }
.numbers td:nth-child(2) { font-weight: 700; }
.foot { border-top: 1px solid var(--line); color: var(--muted); font-size: 14px; }
.foot p { max-width: 1240px; margin: 0 auto; padding: 24px; }
@media (max-width: 900px) {
  .word { font-size: clamp(76px, 24vw, 150px); }
  .hero { grid-template-columns: 1fr; gap: 32px; padding: 40px 0 48px; }
  .points { grid-template-columns: 1fr; gap: 28px; }
}
</style>"""

HERO = '''let dog = {name: "Rambo", tricks: ["sit", "shake"]};
push(dog.tricks, "roll over");

fn show_off(d) {
    for trick in d.tricks {
        print("{d.name} can {trick}!");
    }
    return len(d.tricks);
}

let count = show_off(dog);
print("That's {count} tricks. Good boy.");'''

def build_landing():
    body = bar(0) + LANDING.replace("{hero}", html.escape(HERO, quote=False))
    write(DIST / "index.html", page("Rambo, a small scripting language", body, 0, head_extra=LANDING_CSS,
                                    scripts='<script>Rambo.wireRunnables("playground/index.html");</script>'))

def examples_json():
    files = {p.stem: read(p) for p in sorted((ROOT / "examples").glob("*.rmb"))}
    order = [s for s in EXAMPLE_ORDER if s in files] + [s for s in files if s not in EXAMPLE_ORDER]
    return json.dumps([{"name": s.replace("_", " "), "code": files[s]} for s in order]).replace("</", "<\\/")

def build_playground(wasm_bytes):
    tpl = read(WEB / "playground.html")
    boot = f"<script>window.RAMBO_EXAMPLES = {examples_json()};</script>"
    site = tpl.replace("<!--SITE_CSS-->", '<link rel="stylesheet" href="../assets/site.css">') \
              .replace("<!--BOOT-->", boot + '\n<script>window.RAMBO_WASM = "../rambo.wasm";</script>') \
              .replace("<!--RAMBO_JS-->", '<script src="../assets/rambo.js"></script>')
    write(DIST / "playground" / "index.html", site)
    # a single self-contained file: CSS, JS and the wasm binary all inlined
    b64 = base64.b64encode(wasm_bytes).decode()
    solo = tpl.replace("<!--SITE_CSS-->", "<style>\n" + read(WEB / "assets" / "site.css") + "\n</style>") \
              .replace("<!--BOOT-->", boot + f'\n<script>window.RAMBO_WASM_B64 = "{b64}";</script>') \
              .replace("<!--RAMBO_JS-->", "<script>\n" + read(WEB / "assets" / "rambo.js") + "\n</script>") \
              .replace('href="../index.html"', 'href="#"') \
              .replace('href="../docs/getting-started.html"', 'href="https://github.com/CodingWithTaj/rambo#readme"') \
              .replace('href="../docs/stdlib.html"', 'href="https://github.com/CodingWithTaj/rambo/blob/main/docs/stdlib.md"')
    write(DIST / "rambo-playground.html", solo)

def main():
    wasm = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_WASM
    if not wasm.exists():
        sys.exit(f"error: {wasm} not found.\nBuild it first:  cargo build --lib --release --target wasm32-unknown-unknown")
    if DIST.exists():
        shutil.rmtree(DIST)
    wasm_bytes = wasm.read_bytes()
    (DIST / "assets").mkdir(parents=True)
    (DIST / "rambo.wasm").write_bytes(wasm_bytes)
    for f in (WEB / "assets").iterdir():
        shutil.copy(f, DIST / "assets" / f.name)
    build_landing()
    build_docs()
    build_playground(wasm_bytes)
    print(f"built {DIST} ({len(wasm_bytes) // 1024} KB wasm, {len(PAGES)} doc pages)")

if __name__ == "__main__":
    main()
