/* Rambo in the browser: WebAssembly loader, syntax highlighter, runnable examples.
 * Load with window.RAMBO_WASM set to a URL (or RAMBO_WASM_B64 to inline bytes). */
(function () {
  "use strict";
  let ex = null;
  const enc = new TextEncoder(), dec = new TextDecoder();

  async function init() {
    let bytes;
    if (window.RAMBO_WASM_B64) {
      const bin = atob(window.RAMBO_WASM_B64);
      bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    } else {
      const res = await fetch(window.RAMBO_WASM || "rambo.wasm");
      if (!res.ok) throw new Error("couldn't load rambo.wasm (" + res.status + ")");
      bytes = new Uint8Array(await res.arrayBuffer());
    }
    const { instance } = await WebAssembly.instantiate(bytes, { env: { now: () => performance.now() } });
    ex = instance.exports;
  }

  function result() {
    const view = new Uint8Array(ex.memory.buffer, ex.result_ptr(), ex.result_len());
    return JSON.parse(dec.decode(view));
  }

  // Copy a string into wasm memory and call fn(ptr, len, ...extra).
  function call(fn, text, ...extra) {
    const bytes = enc.encode(text);
    const ptr = ex.alloc(bytes.length);
    new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
    fn(ptr, bytes.length, ...extra);
    return result();
  }

  const LIMIT = 50e6;
  const Rambo = {
    ready: null,
    run: (src) => call(ex.run, src, LIMIT),
    disassemble: (src) => call(ex.disassemble, src),
    debugStart: (src) => call(ex.dbg_start, src, LIMIT),
    debugStep: (mode) => { ex.dbg_step(mode); return result(); },
    setBreakpoints: (lines) => call(ex.dbg_breakpoints, lines.join(",")),
    debugStop: () => ex.dbg_stop(),
  };

  // ---- syntax highlighting -------------------------------------------
  const KEYWORDS = new Set("let fn return if else while for in break continue true false nil".split(" "));
  const BUILTINS = new Set(("print type str num len assert error clock gc memory floor ceil round abs sqrt pow min max " +
    "random random_int seed push pop insert remove slice contains index_of reverse sort join range split upper lower " +
    "trim replace starts_with ends_with chars ord chr keys values items has map filter reduce each find any all sum sort_by PI").split(" "));
  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const span = (cls, s) => '<span class="tk-' + cls + '">' + esc(s) + "</span>";

  function highlightString(s) {
    // strings in tan, with {interpolated} code shown as code
    let out = "", i = 0, lit = "";
    while (i < s.length) {
      if (s[i] === "\\" && i + 1 < s.length) { lit += s.slice(i, i + 2); i += 2; continue; }
      if (s[i] === "{" && i > 0) {
        let depth = 1, j = i + 1;
        while (j < s.length - 1 && depth > 0) { if (s[j] === "{") depth++; else if (s[j] === "}") depth--; if (depth) j++; }
        if (depth === 0) {
          out += span("s", lit) + span("i", "{") + highlight(s.slice(i + 1, j)) + span("i", "}");
          lit = ""; i = j + 1; continue;
        }
      }
      lit += s[i++];
    }
    return out + span("s", lit);
  }

  const TOKEN_SOURCE = /(\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$))|("(?:[^"\\\n]|\\.)*"?)|(\d[\d_]*(?:\.\d[\d_]*)?)|([A-Za-z_\u00C0-\uFFFF][\w\u00C0-\uFFFF]*)|([\s\S])/.source;
  function highlight(src) {
    // a fresh regex per call: highlightString() calls back into highlight()
    // for {interpolated} code, and a shared /g regex would lose its place
    const TOKEN = new RegExp(TOKEN_SOURCE, "g");
    let out = "", m, prevWord = "";
    while ((m = TOKEN.exec(src))) {
      if (m[1]) out += span("c", m[1]);
      else if (m[2]) out += highlightString(m[2]);
      else if (m[3]) out += span("n", m[3]);
      else if (m[4]) {
        const w = m[4];
        const next = src.slice(TOKEN.lastIndex).match(/^\s*\(/);
        if (KEYWORDS.has(w)) out += span("k", w);
        else if (prevWord === "fn" || (next && !BUILTINS.has(w))) out += span("f", w);
        else if (BUILTINS.has(w)) out += span("b", w);
        else out += esc(w);
        prevWord = w;
        continue;
      } else out += esc(m[5]);
      if (m[5] && !/\s/.test(m[5])) prevWord = "";
    }
    return out;
  }
  Rambo.highlight = highlight;

  // ---- runnable examples in the docs --------------------------------
  function outputHtml(r) {
    let html = esc(r.output || "");
    if (!r.ok) html += '<span class="err">' + esc(r.error) + "</span>";
    return html || '<span class="tk-c">(no output)</span>';
  }
  Rambo.outputHtml = outputHtml;

  function wireRunnables(playgroundUrl) {
    document.querySelectorAll(".runnable").forEach((box) => {
      const code = box.querySelector("code");
      const src = code.textContent;
      code.innerHTML = highlight(src);
      const out = box.querySelector(".out");
      box.querySelector(".run").addEventListener("click", async () => {
        await Rambo.ready;
        out.innerHTML = outputHtml(Rambo.run(src));
      });
      const open = box.querySelector(".open");
      if (open) open.href = playgroundUrl + "#code=" + encodeURIComponent(src);
    });
  }
  Rambo.wireRunnables = wireRunnables;

  Rambo.ready = init().catch((e) => {
    console.error(e);
    document.querySelectorAll(".runnable .run").forEach((b) => { b.disabled = true; b.title = String(e.message || e); });
    throw e;
  });
  window.Rambo = Rambo;
})();
