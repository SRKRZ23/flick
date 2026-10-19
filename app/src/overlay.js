"use strict";
// The wheel that opens at the cursor over Finder (see src-tauri/src/gesture.rs).
// Rust opens/closes it, streams the cursor and button state, and sends the files
// (Finder's selection). The wheel's centre is transparent so the file under the
// cursor can be grabbed and dragged onto a slice; slices are also clickable.
(function () {
  var T = window.__TAURI__ || {};
  var invoke = T.core && T.core.invoke;
  var listen = T.event && T.event.listen;
  if (!invoke || !listen) return;

  var SVGNS = "http://www.w3.org/2000/svg";
  var SIZE = 420, R = 172, r = 70; // must match gesture.rs SIZE
  var host = document.getElementById("wheelHost");
  var bubble = document.getElementById("bubble");
  var pill = document.getElementById("pill");

  var mode = "convert", paths = [], plan = null, slices = [], hot = -1, busy = false, planning = false;
  var dropped = false, pendingPt = null, nofiles = false;

  function log(msg) { try { invoke("log", { msg: "overlay: " + msg }); } catch (e) {} }

  function itemsForMode() {
    if (!plan) return [];
    if (mode === "convert") return plan.convert.map(function (x) { return { label: x.toUpperCase(), value: x }; });
    if (mode === "tools") return plan.tools.map(function (x) { return { label: x, value: x }; });
    return plan.actions.map(function (x) { return { label: x, value: x }; });
  }
  function modeName() { return mode === "convert" ? "Convert" : mode === "tools" ? "Tools" : "Actions"; }
  function pillText() {
    if (plan) return modeName() + " · " + paths.length + (paths.length === 1 ? " file" : " files");
    if (nofiles) return "Select a file in Finder first";
    if (paths.length || planning) return "Reading…";
    return modeName() + " · reading selection…";
  }
  function arcPath(a0, a1) {
    var large = (a1 - a0) > Math.PI ? 1 : 0;
    var x0 = Math.cos(a0) * R, y0 = Math.sin(a0) * R, x1 = Math.cos(a1) * R, y1 = Math.sin(a1) * R;
    var ix1 = Math.cos(a1) * r, iy1 = Math.sin(a1) * r, ix0 = Math.cos(a0) * r, iy0 = Math.sin(a0) * r;
    return "M" + x0 + " " + y0 + "A" + R + " " + R + " 0 " + large + " 1 " + x1 + " " + y1 +
           "L" + ix1 + " " + iy1 + "A" + r + " " + r + " 0 " + large + " 0 " + ix0 + " " + iy0 + "Z";
  }

  function render() {
    host.innerHTML = ""; slices = []; hot = -1;
    pill.textContent = pillText();
    var items = itemsForMode(), n = items.length;
    var svg = document.createElementNS(SVGNS, "svg");
    svg.setAttribute("class", "wheel");
    svg.setAttribute("viewBox", (-SIZE / 2) + " " + (-SIZE / 2) + " " + SIZE + " " + SIZE);
    if (n) {
      var step = (Math.PI * 2) / n;
      for (var i = 0; i < n; i++) {
        var a0 = -Math.PI / 2 + i * step + 0.03, a1 = -Math.PI / 2 + (i + 1) * step - 0.03;
        var g = document.createElementNS(SVGNS, "g"); g.setAttribute("class", "slice");
        var p = document.createElementNS(SVGNS, "path"); p.setAttribute("class", "slice-bg"); p.setAttribute("d", arcPath(a0, a1));
        g.appendChild(p);
        var mid = (a0 + a1) / 2, lr = (R + r) / 2;
        var tx = document.createElementNS(SVGNS, "text");
        tx.setAttribute("class", "slice-label"); tx.setAttribute("text-anchor", "middle");
        tx.setAttribute("x", (Math.cos(mid) * lr).toFixed(1)); tx.setAttribute("y", (Math.sin(mid) * lr + 5).toFixed(1));
        tx.textContent = items[i].label; g.appendChild(tx);
        (function (idx) { g.addEventListener("click", function () { pick(idx); }); })(i);
        svg.appendChild(g);
        slices.push({ g: g, a0: a0, a1: a1, value: items[i].value });
      }
    } else {
      // No slices yet: just a thin ring so the user sees where the wheel is.
      var ring = document.createElementNS(SVGNS, "circle");
      ring.setAttribute("class", "wheel-ring"); ring.setAttribute("r", (R + r) / 2); ring.setAttribute("cx", 0); ring.setAttribute("cy", 0);
      svg.appendChild(ring);
    }
    // The centre stays unpainted: clicks there fall through to Finder (grab the file).
    host.appendChild(svg);
  }

  function indexAt(pt) {
    if (!pt || !slices.length) return -1;
    var x = pt.x - SIZE / 2, y = pt.y - SIZE / 2;
    var dist = Math.sqrt(x * x + y * y);
    if (dist < r - 6 || dist > R + 24) return -1;
    var ang = Math.atan2(y, x);
    for (var i = 0; i < slices.length; i++) {
      var a = ang, a0 = slices[i].a0, a1 = slices[i].a1;
      if (a < a0 - 0.1) a += Math.PI * 2;
      if (a >= a0 - 0.06 && a <= a1 + 0.06) return i;
    }
    return -1;
  }
  function highlight(i) {
    if (i === hot) return;
    hot = i;
    slices.forEach(function (s, k) { s.g.classList.toggle("hot", k === i); });
  }
  function fromPhysical(pos) {
    if (!pos) return null;
    var dpr = window.devicePixelRatio || 1;
    return { x: pos.x / dpr, y: pos.y / dpr };
  }
  function hover(pt) {
    if (!pt) return;
    if (!plan) { pendingPt = pt; return; }
    highlight(indexAt(pt));
  }

  function showBubble(ok, main, sub) {
    host.innerHTML = ""; pill.textContent = "";
    bubble.className = "bubble " + (ok ? "okk" : "errr");
    bubble.innerHTML = '<span class="b-dot"></span><span class="b-main">' + esc(main) + "</span>" +
      (sub ? '<div class="b-sub">' + esc(sub) + "</div>" : "");
  }
  function esc(s) { return String(s).replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; }); }
  function base(p) { return p.replace(/^.*[\\/]/, ""); }

  function reset() {
    mode = "convert"; paths = []; plan = null; busy = false; planning = false; dropped = false; pendingPt = null; nofiles = false;
    bubble.className = "bubble hidden"; render();
  }

  function setFiles(list, source) {
    if (!list || !list.length || planning || plan) return;
    paths = list; planning = true; nofiles = false; render();
    if (source === "os") { try { invoke("files_received"); } catch (e) {} }
    invoke("plan", { paths: paths }).then(function (p) {
      plan = p; planning = false; render();
      if (pendingPt) highlight(indexAt(pendingPt));
      log("plan ok (" + source + "): " + paths.length + " file(s), convert=" + (p.convert || []).join(","));
    }).catch(function (e) { planning = false; log("plan FAILED: " + e); });
  }

  function pick(i) {
    if (busy || i < 0 || !paths.length || !slices[i]) return;
    busy = true;
    var value = slices[i].value, call;
    log("pick " + mode + " → " + value);
    if (mode === "convert") call = invoke("convert", { paths: paths, to: value });
    else if (mode === "tools") call = invoke("run_tool", { paths: paths, tool: value });
    else if (value === "Copy path") { try { navigator.clipboard.writeText(paths.join("\n")); } catch (e) {} call = Promise.resolve([{ ok: true, outputs: paths }]); }
    else call = invoke("action", { paths: paths, action: value }).then(function (o) { return [o]; });
    showBubble(true, "Working…", base(paths[0]));
    call.then(function (res) {
      var outs = [], err = null;
      (Array.isArray(res) ? res : [res]).forEach(function (o) { if (o && o.ok) outs = outs.concat(o.outputs || []); else if (o && o.error && !err) err = o.error; });
      if (outs.length) showBubble(true, (mode === "convert" ? "Converted" : "Done") + " · " + outs.length, outs.slice(0, 2).map(base).join(", "));
      else showBubble(false, "Couldn't do that", err || "No output");
      log(outs.length ? "ok: " + outs.join(", ") : "failed: " + err);
      setTimeout(function () { invoke("hide_wheel"); }, outs.length ? 1300 : 2200);
    }).catch(function (e) {
      showBubble(false, "Error", String(e)); log("error: " + e);
      setTimeout(function () { invoke("hide_wheel"); }, 2200);
    });
  }

  // ---- from Rust: open/close, files, cursor, button, modifier ----
  listen("wheel", function (e) {
    var p = e.payload || {};
    switch (p.state) {
      case "open":    reset(); if (p.mode) mode = p.mode; render(); log("open mode=" + mode); break;
      case "files":   setFiles(p.paths || [], "finder selection"); break;
      case "nofiles": if (!plan && !planning) { nofiles = true; render(); } break;
      case "mode":    if (p.mode && p.mode !== mode && !busy) { mode = p.mode; render(); } break;
      case "cursor":  if (!busy) hover(p); break;
      case "release":
        // The OS drop usually lands within a few ms of the button-up; give it 400 ms,
        // then fall back to the slice the cursor was released on.
        var rel = { x: p.x, y: p.y };
        setTimeout(function () {
          if (busy || dropped) return;
          var i = indexAt(rel);
          if (i >= 0 && plan) { log("release → slice " + i); pick(i); }
        }, 400);
        break;
      case "dismiss": if (!busy) invoke("hide_wheel"); break;
      case "close":   reset(); break;
    }
  });

  // ---- Finder's drag session entering this window ----
  // Positions in these events use a different coordinate base than our window, so
  // hover and the drop target always come from the Rust cursor stream (`hot`).
  listen("tauri://drag-enter", function (e) {
    var pl = e.payload || {};
    log("drag-enter " + ((pl.paths || []).length) + " paths");
    setFiles(pl.paths || [], "os");
  });
  listen("tauri://drag-drop", function (e) {
    var pl = e.payload || {};
    if (pl.paths && pl.paths.length && !plan && !planning) setFiles(pl.paths, "os");
    var i = hot;
    if (i < 0 && plan && pendingPt) i = indexAt(pendingPt);
    log("drop → slice " + i + (plan ? "" : " (plan pending)"));
    if (plan) { if (i >= 0) { dropped = true; pick(i); } return; }   // else: the release fallback may still pick
    // Plan still loading: remember the drop point and pick once the slices exist.
    var pt = pendingPt, tries = 0;
    (function waitPlan() {
      if (plan) { var j = indexAt(pt); if (j >= 0) { dropped = true; pick(j); } }
      else if (++tries < 40) setTimeout(waitPlan, 50);
    })();
  });

  render();
})();
