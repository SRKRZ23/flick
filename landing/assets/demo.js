(function () {
  "use strict";
  var stage = document.getElementById("stage");
  var chip = document.getElementById("chip");
  var chipName = document.getElementById("chip-name");
  var chipIco = document.getElementById("chip-ico");
  var hint = document.getElementById("hint");
  var hintText = document.getElementById("hint-text");
  var keycaps = document.getElementById("keycaps");
  if (!stage || !chip) return;

  var SRC = { name: "photo", ext: "jpg", ico: "🖼️" };
  var WHEELS = {
    convert: { center: "Convert", items: [
      { l: "PNG", ico: "🖼️" }, { l: "WebP", ico: "🖼️" },
      { l: "PDF", ico: "📄" }, { l: "HEIC", ico: "🖼️" },
      { l: "TIFF", ico: "🖼️" }, { l: "AVIF", ico: "🖼️" } ] },
    tools: { center: "Tools", items: [
      { l: "Compress" }, { l: "Resize" }, { l: "Crop" }, { l: "Rotate" } ] },
    actions: { center: "Actions", items: [
      { l: "Copy" }, { l: "Move" }, { l: "Reveal" }, { l: "Copy Path" } ] }
  };

  var SVGNS = "http://www.w3.org/2000/svg";
  var dragging = false, mode = "convert", wheel = null, slices = [], hotIndex = -1, busy = false;
  var R = 118, r = 46, PAD = 16;

  function setMode(m) {
    mode = m;
    Array.prototype.forEach.call(keycaps.querySelectorAll(".keycap"), function (k) {
      k.classList.toggle("on", k.getAttribute("data-mode") === m);
    });
  }
  function setHint(t, show) {
    if (t) hintText.textContent = t;
    hint.style.opacity = show ? "1" : "0";
  }
  function arcPath(a0, a1) {
    var large = (a1 - a0) > Math.PI ? 1 : 0;
    var x0 = Math.cos(a0) * R, y0 = Math.sin(a0) * R, x1 = Math.cos(a1) * R, y1 = Math.sin(a1) * R;
    var ix1 = Math.cos(a1) * r, iy1 = Math.sin(a1) * r, ix0 = Math.cos(a0) * r, iy0 = Math.sin(a0) * r;
    return "M" + x0 + " " + y0 + "A" + R + " " + R + " 0 " + large + " 1 " + x1 + " " + y1 +
           "L" + ix1 + " " + iy1 + "A" + r + " " + r + " 0 " + large + " 0 " + ix0 + " " + iy0 + "Z";
  }
  function buildWheel() {
    if (wheel) wheel.remove();
    var items = WHEELS[mode].items, n = items.length, step = (Math.PI * 2) / n;
    var size = (R + PAD) * 2;
    wheel = document.createElementNS(SVGNS, "svg");
    wheel.setAttribute("class", "wheel");
    wheel.setAttribute("width", size); wheel.setAttribute("height", size);
    wheel.setAttribute("viewBox", (-(R + PAD)) + " " + (-(R + PAD)) + " " + size + " " + size);
    slices = [];
    for (var i = 0; i < n; i++) {
      var a0 = -Math.PI / 2 + i * step + 0.04, a1 = -Math.PI / 2 + (i + 1) * step - 0.04;
      var g = document.createElementNS(SVGNS, "g");
      g.setAttribute("class", "slice"); g.setAttribute("data-i", i);
      var p = document.createElementNS(SVGNS, "path");
      p.setAttribute("class", "slice-bg"); p.setAttribute("d", arcPath(a0, a1));
      g.appendChild(p);
      var mid = (a0 + a1) / 2, lr = (R + r) / 2;
      var tx = document.createElementNS(SVGNS, "text");
      tx.setAttribute("class", "slice-label"); tx.setAttribute("text-anchor", "middle");
      tx.setAttribute("x", Math.cos(mid) * lr); tx.setAttribute("y", Math.sin(mid) * lr + 4);
      tx.textContent = items[i].l;
      g.appendChild(tx);
      (function (idx) {
        g.addEventListener("pointerdown", function (e) { e.stopPropagation(); pick(idx); });
        g.addEventListener("click", function (e) { e.stopPropagation(); pick(idx); });
      })(i);
      wheel.appendChild(g);
      slices.push({ g: g, a0: a0, a1: a1 });
    }
    var c = document.createElementNS(SVGNS, "circle");
    c.setAttribute("class", "wheel-center"); c.setAttribute("r", r - 2);
    c.setAttribute("cx", 0); c.setAttribute("cy", 0);
    wheel.appendChild(c);
    var cl = document.createElementNS(SVGNS, "text");
    cl.setAttribute("class", "wheel-center-label"); cl.setAttribute("x", 0); cl.setAttribute("y", 4);
    cl.textContent = WHEELS[mode].center;
    wheel.appendChild(cl);
    stage.appendChild(wheel);
  }
  function showWheelAt(x, y) {
    buildWheel();
    wheel.style.left = x + "px"; wheel.style.top = y + "px";
    requestAnimationFrame(function () { wheel.classList.add("show"); });
    wheel._cx = x; wheel._cy = y;
    setHint("Drop on a format", true);
  }
  function hideWheel() {
    if (wheel) { wheel.classList.remove("show"); var w = wheel; wheel = null; setTimeout(function () { if (w) w.remove(); }, 180); }
    hotIndex = -1;
  }
  function highlight(px, py) {
    if (!wheel) return;
    var dx = px - wheel._cx, dy = py - wheel._cy, dist = Math.sqrt(dx * dx + dy * dy);
    var ang = Math.atan2(dy, dx); hotIndex = -1;
    if (dist >= r - 6 && dist <= R + 14) {
      for (var i = 0; i < slices.length; i++) {
        var a0 = slices[i].a0, a1 = slices[i].a1, a = ang;
        if (a < a0 - 0.1) a += Math.PI * 2;
        if (a >= a0 - 0.06 && a <= a1 + 0.06) { hotIndex = i; break; }
      }
    }
    slices.forEach(function (s, i) { s.g.classList.toggle("hot", i === hotIndex); });
  }
  function pick(i) {
    if (busy) return;
    var item = WHEELS[mode].items[i];
    hideWheel();
    if (mode === "convert") convertTo(item);
    else flash(item.l);
    endDrag(true);
  }
  function flash(label) {
    busy = true; setHint(label + " ✓", true); chip.classList.add("done");
    setTimeout(reset, 1400);
  }
  function convertTo(item) {
    busy = true; setHint("Converting…", true);
    var ring = document.createElementNS(SVGNS, "svg");
    ring.setAttribute("class", "conv-ring show"); ring.setAttribute("width", 70); ring.setAttribute("height", 70);
    ring.setAttribute("viewBox", "0 0 70 70");
    ring.innerHTML = '<circle class="ring-track" cx="35" cy="35" r="30"/><circle class="ring-prog" cx="35" cy="35" r="30" stroke-dasharray="188" stroke-dashoffset="188"/>';
    ring.style.left = chip.offsetLeft + chip.offsetWidth / 2 + "px";
    ring.style.top = chip.offsetTop + chip.offsetHeight / 2 - 44 + "px";
    stage.appendChild(ring);
    var prog = ring.querySelector(".ring-prog"), t0 = performance.now(), DUR = 620;
    (function step(t) {
      var k = Math.min(1, (t - t0) / DUR);
      prog.setAttribute("stroke-dashoffset", String(188 * (1 - k)));
      if (k < 1) requestAnimationFrame(step);
      else {
        chipName.textContent = SRC.name + "." + item.l.toLowerCase();
        chipIco.textContent = item.ico || SRC.ico;
        chip.classList.add("done"); setHint("Done ✓ " + SRC.name + "." + item.l.toLowerCase(), true);
        setTimeout(function () { ring.remove(); }, 300);
        setTimeout(reset, 1700);
      }
    })(t0);
  }
  function reset() {
    chipName.textContent = SRC.name + "." + SRC.ext; chipIco.textContent = SRC.ico;
    chip.classList.remove("done"); chip.style.left = ""; chip.style.top = "";
    chip.style.transform = "translate(-50%,-50%)"; setMode("convert"); busy = false;
    setHint("Drag the file", true);
  }

  // ---- pointer drag ----
  function moveChip(px, py) {
    chip.style.left = px + "px"; chip.style.top = py + "px"; chip.style.transform = "translate(-50%,-50%)";
  }
  function stagePoint(e) { var b = stage.getBoundingClientRect(); return { x: e.clientX - b.left, y: e.clientY - b.top }; }
  chip.addEventListener("pointerdown", function (e) {
    if (busy) return; e.preventDefault(); dragging = true; chip.classList.add("dragging");
    try { chip.setPointerCapture(e.pointerId); } catch (_) {}
    var p = stagePoint(e); moveChip(p.x, p.y);
    setHint("Hold ⇧ ⌘ ⌃ for a wheel", true);
  });
  stage.addEventListener("pointermove", function (e) {
    if (!dragging) return; var p = stagePoint(e); moveChip(p.x, p.y);
    // wheel stays anchored where it opened; only highlight the slice under the cursor
    if (wheel) highlight(p.x, p.y);
  });
  function up() {
    if (!dragging) return;
    if (wheel && hotIndex >= 0) pick(hotIndex);
    else { hideWheel(); endDrag(false); }
  }
  stage.addEventListener("pointerup", up);
  stage.addEventListener("pointercancel", up);
  function endDrag(converted) {
    dragging = false; chip.classList.remove("dragging");
    if (!converted && !busy) { chip.style.left = ""; chip.style.top = ""; chip.style.transform = "translate(-50%,-50%)"; setHint("Drag the file", true); }
  }

  // ---- keyboard modifiers open the wheel while dragging ----
  function modeFromEvent(e) { if (e.metaKey) return "tools"; if (e.ctrlKey) return "actions"; if (e.shiftKey) return "convert"; return null; }
  window.addEventListener("keydown", function (e) {
    if (!dragging || busy) return;
    var m = modeFromEvent(e); if (!m) return; e.preventDefault();
    if (!wheel || m !== mode) { setMode(m); var p = { x: parseFloat(chip.style.left) || stage.clientWidth / 2, y: parseFloat(chip.style.top) || stage.clientHeight / 2 }; showWheelAt(p.x, p.y); highlight(p.x, p.y); }
  });
  window.addEventListener("keyup", function (e) {
    if (!dragging) return;
    if (!e.shiftKey && !e.metaKey && !e.ctrlKey && wheel) { hideWheel(); setHint("Hold ⇧ ⌘ ⌃ for a wheel", true); }
  });

  // ---- click / tap fallback (phones, no-modifier) ----
  chip.addEventListener("click", function (e) {
    if (dragging || busy) return; e.preventDefault();
    setMode("convert"); showWheelAt(stage.clientWidth / 2, stage.clientHeight * 0.52);
    dragging = true; // let a slice click resolve
  });
  chip.addEventListener("keydown", function (e) {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); chip.click(); }
  });
  Array.prototype.forEach.call(keycaps.querySelectorAll(".keycap"), function (k) {
    k.addEventListener("click", function () {
      if (busy) return; setMode(k.getAttribute("data-mode"));
      showWheelAt(stage.clientWidth / 2, stage.clientHeight * 0.52); dragging = true;
    });
  });

  // idle nudge
  var nudged = false;
  setTimeout(function () { if (!nudged && !busy) setHint("Drag the file, then hold ⇧", true); }, 3500);
  chip.addEventListener("pointerdown", function () { nudged = true; });
  setMode("convert");
})();
