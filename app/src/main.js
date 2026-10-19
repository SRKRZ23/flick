"use strict";
// Flick UI. Talks to the Rust host (flick-engine) over Tauri's invoke bridge.
// withGlobalTauri exposes everything on window.__TAURI__ — no bundler needed.
(function () {
  var T = window.__TAURI__ || {};
  var invoke = T.core && T.core.invoke;
  var listen = T.event && T.event.listen;

  var SVGNS = "http://www.w3.org/2000/svg";
  var R = 150, r = 62; // wheel outer / inner radius

  var files = [];        // dropped absolute paths
  var plan = null;       // DropPlan from the engine
  var mode = "convert";  // convert | tools | actions
  var busy = false;

  var EXT_ICON = {
    jpg:"🖼️", png:"🖼️", webp:"🖼️", heic:"🖼️", avif:"🖼️", tiff:"🖼️", gif:"🖼️", bmp:"🖼️", svg:"🖼️",
    mp3:"🎵", m4a:"🎵", wav:"🎵", flac:"🎵", ogg:"🎵", opus:"🎵", aiff:"🎵",
    mp4:"🎬", mov:"🎬", mkv:"🎬", webm:"🎬", avi:"🎬",
    pdf:"📄", docx:"📝", doc:"📝", odt:"📝", rtf:"📝", txt:"📝", md:"📝", html:"🌐", epub:"📚",
    xlsx:"📊", xls:"📊", csv:"📊", ods:"📊",
    pptx:"📑", ppt:"📑", odp:"📑",
    zip:"🗜️", "7z":"🗜️", tar:"🗜️", gz:"🗜️"
  };
  function iconFor(p) {
    var m = /\.([a-z0-9]+)$/i.exec(p); return (m && EXT_ICON[m[1].toLowerCase()]) || "📁";
  }
  function baseName(p) { return p.replace(/^.*[\\/]/, ""); }

  // ---------- theme ----------
  try {
    var t = localStorage.getItem("flick-theme");
    if (t) document.documentElement.setAttribute("data-theme", t);
  } catch (e) {}
  document.getElementById("theme").addEventListener("click", function () {
    var d = document.documentElement;
    var n = d.getAttribute("data-theme") === "light" ? "dark" : "light";
    d.setAttribute("data-theme", n);
    try { localStorage.setItem("flick-theme", n); } catch (e) {}
  });
  document.getElementById("ghLink").addEventListener("click", function (e) {
    e.preventDefault();
    if (T.opener && T.opener.openUrl) T.opener.openUrl("https://github.com/SRKRZ23/flick");
  });

  // ---------- engine status ----------
  var ENG = {
    magick:["Images","imagemagick"], ffmpeg:["Audio / Video","ffmpeg"],
    soffice:["Documents","libreoffice"], pandoc:["Markup","pandoc"],
    pdftoppm:["PDF → image","poppler"], mutool:["PDF text","mupdf-tools"],
    qpdf:["PDF tools","qpdf"], gs:["PDF compress","ghostscript"], sevenzip:["Archives","sevenzip"]
  };
  function showEngines() {
    if (!invoke) return;
    invoke("detect").then(function (st) {
      var host = document.getElementById("engines");
      host.innerHTML = Object.keys(ENG).map(function (k) {
        var on = !!st[k];
        return '<span class="eng ' + (on ? "ok" : "no") + '" title="' +
          (on ? "installed" : "missing — brew install " + ENG[k][1]) + '">' + ENG[k][0] + "</span>";
      }).join("");
    }).catch(function () {});
  }

  // ---------- drag & drop ----------
  if (listen) {
    listen("tauri://drag-enter", function () { document.body.classList.add("dragging"); });
    listen("tauri://drag-leave", function () { document.body.classList.remove("dragging"); });
    listen("tauri://drag-drop", function (e) {
      document.body.classList.remove("dragging");
      var paths = (e && e.payload && e.payload.paths) || [];
      if (paths.length) onFiles(paths);
    });
    // Files sent from Finder → "Open With → Flick".
    listen("open-files", function (e) {
      var paths = (e && e.payload) || [];
      if (paths.length) onFiles(paths);
    });
  }

  function onFiles(paths) {
    files = paths; busy = false; mode = "convert";
    if (!invoke) return;
    invoke("plan", { paths: paths }).then(function (p) {
      plan = p; renderStage();
    }).catch(function (err) { toast(false, "Could not read files", String(err)); });
  }

  // ---------- stage ----------
  function show(id, on) { document.getElementById(id).classList.toggle("hidden", !on); }
  function renderStage() {
    show("dropzone", false); show("stage", true);
    var fh = document.getElementById("files");
    var shown = files.slice(0, 6).map(function (p) {
      return '<div class="fchip"><span class="ico">' + iconFor(p) +
        '</span><span class="nm">' + escapeHtml(baseName(p)) + "</span></div>";
    }).join("");
    if (files.length > 6) shown += '<div class="fchip"><span class="cat">+' + (files.length - 6) + " more</span></div>";
    fh.innerHTML = shown;
    setMode("convert");
  }

  function setMode(m) {
    mode = m;
    Array.prototype.forEach.call(document.querySelectorAll(".mode"), function (b) {
      b.classList.toggle("on", b.getAttribute("data-mode") === m);
    });
    buildWheel();
  }

  function itemsForMode() {
    if (!plan) return [];
    if (mode === "convert") return plan.convert.map(function (x) { return { label: x.toUpperCase(), value: x }; });
    if (mode === "tools") return plan.tools.map(function (x) { return { label: x, value: x }; });
    return plan.actions.map(function (x) { return { label: x, value: x }; });
  }
  function centerText() {
    return mode === "convert" ? ["Convert", "to…"] : mode === "tools" ? ["Tools", plan ? plan.from_ext.toUpperCase() : ""] : ["Actions", ""];
  }

  // ---------- radial wheel ----------
  function arcPath(a0, a1) {
    var large = (a1 - a0) > Math.PI ? 1 : 0;
    var x0 = Math.cos(a0) * R, y0 = Math.sin(a0) * R, x1 = Math.cos(a1) * R, y1 = Math.sin(a1) * R;
    var ix1 = Math.cos(a1) * r, iy1 = Math.sin(a1) * r, ix0 = Math.cos(a0) * r, iy0 = Math.sin(a0) * r;
    return "M" + x0 + " " + y0 + "A" + R + " " + R + " 0 " + large + " 1 " + x1 + " " + y1 +
           "L" + ix1 + " " + iy1 + "A" + r + " " + r + " 0 " + large + " 0 " + ix0 + " " + iy0 + "Z";
  }
  function buildWheel() {
    var host = document.getElementById("wheelHost");
    host.innerHTML = "";
    var items = itemsForMode();
    if (!items.length) {
      host.innerHTML = '<div class="empty-note">Nothing available here for this file type. Try another key.</div>';
      return;
    }
    var n = items.length, step = (Math.PI * 2) / n;
    var size = (R + 20) * 2;
    var svg = document.createElementNS(SVGNS, "svg");
    svg.setAttribute("class", "wheel" + (busy ? " busy" : ""));
    svg.setAttribute("viewBox", (-(R + 20)) + " " + (-(R + 20)) + " " + size + " " + size);
    for (var i = 0; i < n; i++) {
      var a0 = -Math.PI / 2 + i * step + 0.03, a1 = -Math.PI / 2 + (i + 1) * step - 0.03;
      var g = document.createElementNS(SVGNS, "g");
      g.setAttribute("class", "slice");
      var p = document.createElementNS(SVGNS, "path");
      p.setAttribute("class", "slice-bg"); p.setAttribute("d", arcPath(a0, a1));
      g.appendChild(p);
      var mid = (a0 + a1) / 2, lr = (R + r) / 2;
      var tx = document.createElementNS(SVGNS, "text");
      tx.setAttribute("class", "slice-label"); tx.setAttribute("text-anchor", "middle");
      tx.setAttribute("x", (Math.cos(mid) * lr).toFixed(1));
      tx.setAttribute("y", (Math.sin(mid) * lr + 5).toFixed(1));
      tx.textContent = items[i].label;
      g.appendChild(tx);
      (function (val) {
        g.addEventListener("click", function () { if (!busy) pick(val); });
      })(items[i].value);
      svg.appendChild(g);
    }
    var c = document.createElementNS(SVGNS, "circle");
    c.setAttribute("class", "wheel-center"); c.setAttribute("r", r - 3); c.setAttribute("cx", 0); c.setAttribute("cy", 0);
    svg.appendChild(c);
    var ct = centerText();
    var cl = document.createElementNS(SVGNS, "text");
    cl.setAttribute("class", "wheel-center-label"); cl.setAttribute("x", 0); cl.setAttribute("y", ct[1] ? -2 : 5);
    cl.textContent = ct[0];
    svg.appendChild(cl);
    if (ct[1]) {
      var cs = document.createElementNS(SVGNS, "text");
      cs.setAttribute("class", "wheel-center-sub"); cs.setAttribute("x", 0); cs.setAttribute("y", 14);
      cs.textContent = ct[1];
      svg.appendChild(cs);
    }
    host.appendChild(svg);
  }

  // ---------- execute ----------
  function pick(value) {
    if (busy || !files.length) return;
    if (mode === "actions" && value === "Copy path") {
      copyToClipboard(files.join("\n"));
      toast(true, "Path copied", files.length + " path(s) on the clipboard");
      return;
    }
    busy = true; buildWheel();
    var call;
    if (mode === "convert") call = invoke("convert", { paths: files, to: value });
    else if (mode === "tools") call = invoke("run_tool", { paths: files, tool: value });
    else call = invoke("action", { paths: files, action: value }).then(function (o) { return [o]; });

    call.then(function (res) {
      busy = false; buildWheel();
      var outcomes = Array.isArray(res) ? res : [res];
      var outs = [], errs = [];
      outcomes.forEach(function (o) {
        if (o && o.ok) outs = outs.concat(o.outputs || []);
        else if (o && o.error) errs.push(o.error);
      });
      if (outs.length) {
        var label = mode === "convert" ? "Converted" : mode === "tools" ? "Done" : value;
        toast(true, label + " · " + outs.length + " file(s)",
          outs.slice(0, 3).map(baseName).join(", ") + (outs.length > 3 ? " …" : ""), outs);
      } else {
        toast(false, "Could not complete", errs[0] || "No output produced");
      }
    }).catch(function (err) {
      busy = false; buildWheel();
      toast(false, "Error", String(err));
    });
  }

  // ---------- toast ----------
  var toastTimer = null;
  function toast(ok, main, sub, revealPaths) {
    var el = document.getElementById("toast");
    var reveal = revealPaths && revealPaths.length
      ? '<button class="t-btn" id="tReveal">Reveal</button>' : "";
    el.className = "toast show " + (ok ? "okk" : "errr");
    el.innerHTML = '<div class="t-row"><span class="t-dot"></span><div><div class="t-main">' +
      escapeHtml(main) + '</div><div class="t-sub">' + escapeHtml(sub || "") + "</div></div>" + reveal + "</div>";
    if (revealPaths && revealPaths.length) {
      var btn = document.getElementById("tReveal");
      if (btn) btn.addEventListener("click", function () {
        invoke("action", { paths: revealPaths, action: "Reveal in Finder" });
      });
    }
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { el.className = "toast hidden"; }, 5200);
  }

  function copyToClipboard(text) {
    try { if (navigator.clipboard) navigator.clipboard.writeText(text); } catch (e) {}
  }
  function escapeHtml(s) {
    return String(s).replace(/[&<>"]/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c];
    });
  }

  // ---------- controls ----------
  Array.prototype.forEach.call(document.querySelectorAll(".mode"), function (b) {
    b.addEventListener("click", function () { if (!busy) setMode(b.getAttribute("data-mode")); });
  });
  document.getElementById("reset").addEventListener("click", function () {
    files = []; plan = null; busy = false;
    show("stage", false); show("dropzone", true);
  });
  // modifier keys switch the active wheel while files are loaded
  window.addEventListener("keydown", function (e) {
    if (!plan || busy) return;
    var m = e.shiftKey ? "convert" : e.metaKey ? "tools" : e.ctrlKey ? "actions" : null;
    if (m && m !== mode) { e.preventDefault(); setMode(m); }
  });

  showEngines();
})();
