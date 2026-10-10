/* Meteorite devlog theme: colour-mode toggle, copy buttons, search, skins, print and the activity graph. Progressive enhancement:
   the site works without JavaScript; the controls below are revealed only when it runs. */
(function () {
  var root = document.documentElement;

  // --- colour mode toggle -------------------------------------------------------
  var toggle = document.getElementById("theme-toggle");
  function current() {
    var t = root.getAttribute("data-theme");
    if (t) return t;
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  function paint() {
    if (!toggle) return;
    var dark = current() === "dark";
    toggle.setAttribute("aria-pressed", String(dark));
    var label = dark ? "Switch to light mode" : "Switch to dark mode";
    toggle.title = label;
    toggle.setAttribute("aria-label", label);
  }
  if (toggle) {
    toggle.hidden = false;
    paint();
    toggle.addEventListener("click", function () {
      var next = current() === "dark" ? "light" : "dark";
      root.setAttribute("data-theme", next);
      try { localStorage.setItem("meteorite-theme", next); } catch (e) {}
      paint();
    });
  }

  // --- copy to clipboard --------------------------------------------------------
  function copy(text, button) {
    function done() {
      var label = button.querySelector("span");
      var old = label ? label.textContent : "";
      button.classList.add("copied");
      if (label) label.textContent = "Copied";
      setTimeout(function () {
        button.classList.remove("copied");
        if (label) label.textContent = old;
      }, 1600);
    }
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(text).then(done, function () {});
    } else {
      var ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand("copy"); done(); } catch (e) {}
      document.body.removeChild(ta);
    }
  }

  document.querySelectorAll("[data-copy]").forEach(function (b) {
    b.addEventListener("click", function () { copy(b.getAttribute("data-copy"), b); });
  });

  document.querySelectorAll(".prose pre").forEach(function (pre) {
    var code = pre.querySelector("code");
    if (!code) return;
    var wrap = document.createElement("div");
    wrap.className = "code-wrap";
    pre.parentNode.insertBefore(wrap, pre);
    wrap.appendChild(pre);
    var b = document.createElement("button");
    b.type = "button";
    b.className = "copy-btn code-copy";
    b.setAttribute("aria-label", "Copy code");
    b.innerHTML = "<span>Copy</span>";
    b.addEventListener("click", function () { copy(code.innerText, b); });
    wrap.appendChild(b);
  });
  // --- search -------------------------------------------------------------------
  // Uses search-index.json (build with `search.enabled: true`). Opens with "/" or Ctrl/Cmd+K.
  var dialog = document.getElementById("search-dialog");
  if (dialog && typeof dialog.showModal === "function") {
    var input = document.getElementById("search-input");
    var list = document.getElementById("search-results");
    var status = document.getElementById("search-status");
    var opener = document.getElementById("search-open");
    var base = dialog.getAttribute("data-base") || "/";
    var docs = null, loading = null, hits = [], active = -1;

    var load = function () {
      if (docs) return Promise.resolve(docs);
      if (!loading) {
        loading = fetch(dialog.getAttribute("data-index"))
          .then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); })
          .then(function (j) {
            docs = j.map(function (d) {
              return { d: d, title: d.title.toLowerCase(), desc: (d.description || "").toLowerCase(),
                       tags: (d.tags || []).join(" ").toLowerCase(), text: (d.text || "").toLowerCase() };
            });
            return docs;
          });
      }
      return loading;
    };

    var tokens = function (q) { return q.toLowerCase().split(/\s+/).filter(Boolean); };

    var run = function (q) {
      var ts = tokens(q);
      if (!ts.length) return [];
      var out = [];
      docs.forEach(function (x) {
        var score = 0;
        for (var i = 0; i < ts.length; i++) {
          var t = ts[i], s = 0;
          if (x.title.indexOf(t) === 0) s += 15; else if (x.title.indexOf(t) > -1) s += 10;
          if (x.tags.indexOf(t) > -1) s += 6;
          if (x.desc.indexOf(t) > -1) s += 4;
          if (x.text.indexOf(t) > -1) s += 1;
          if (!s) return;               // every word must match somewhere
          score += s;
        }
        out.push({ doc: x.d, text: x.text, score: score });
      });
      out.sort(function (a, b) { return b.score - a.score; });
      return out.slice(0, 8);
    };

    var mark = function (text, ts) {
      var frag = document.createDocumentFragment();
      var lower = text.toLowerCase(), pos = 0;
      while (pos < text.length) {
        var best = -1, len = 0;
        ts.forEach(function (t) {
          var i = lower.indexOf(t, pos);
          if (i > -1 && (best === -1 || i < best)) { best = i; len = t.length; }
        });
        if (best === -1) { frag.appendChild(document.createTextNode(text.slice(pos))); break; }
        if (best > pos) frag.appendChild(document.createTextNode(text.slice(pos, best)));
        var m = document.createElement("mark");
        m.textContent = text.slice(best, best + len);
        frag.appendChild(m);
        pos = best + len;
      }
      return frag;
    };

    var snippet = function (doc, ts) {
      var src = doc.description || doc.text || "";
      var lower = src.toLowerCase(), at = -1;
      for (var i = 0; i < ts.length && at < 0; i++) at = lower.indexOf(ts[i]);
      var start = Math.max(0, at - 50);
      var s = src.slice(start, start + 140);
      return (start > 0 ? "…" : "") + s + (start + 140 < src.length ? "…" : "");
    };

    var setActive = function (n) {
      var items = list.children;
      if (!items.length) { active = -1; return; }
      active = (n + items.length) % items.length;
      for (var i = 0; i < items.length; i++) items[i].setAttribute("aria-selected", String(i === active));
      items[active].scrollIntoView({ block: "nearest" });
    };

    var render = function () {
      var q = input.value.trim(), ts = tokens(q);
      list.textContent = "";
      hits = q ? run(q) : [];
      hits.forEach(function (h) {
        var li = document.createElement("li");
        li.setAttribute("role", "option");
        var a = document.createElement("a");
        a.href = base + h.doc.url.replace(/^\/+/, "");
        var t = document.createElement("strong");
        t.appendChild(mark(h.doc.title, ts));
        var s = document.createElement("span");
        s.appendChild(mark(snippet(h.doc, ts), ts));
        a.appendChild(t); a.appendChild(s); li.appendChild(a); list.appendChild(li);
      });
      status.textContent = !q ? "Type to search" : hits.length ? hits.length + (hits.length === 1 ? " result" : " results") : "No results for “" + q + "”";
      setActive(hits.length ? 0 : -1);
    };

    var open = function () {
      if (!dialog.open) dialog.showModal();
      input.focus(); input.select();
      status.textContent = "Loading…";
      load().then(render, function () { status.textContent = "Search is unavailable."; });
    };

    if (opener) { opener.hidden = false; opener.addEventListener("click", open); }
    input.addEventListener("input", function () { if (docs) render(); });
    input.addEventListener("keydown", function (e) {
      if (e.key === "ArrowDown") { e.preventDefault(); setActive(active + 1); }
      else if (e.key === "ArrowUp") { e.preventDefault(); setActive(active - 1); }
      else if (e.key === "Enter" && active > -1) {
        e.preventDefault();
        var a = list.children[active].querySelector("a");
        if (a) window.location.href = a.href;
      }
    });
    dialog.addEventListener("click", function (e) { if (e.target === dialog) dialog.close(); });
    document.addEventListener("keydown", function (e) {
      var tag = (e.target && e.target.tagName) || "";
      var typing = /^(INPUT|TEXTAREA|SELECT)$/.test(tag) || (e.target && e.target.isContentEditable);
      if ((e.key === "/" && !typing && !e.ctrlKey && !e.metaKey) ||
          ((e.key === "k" || e.key === "K") && (e.ctrlKey || e.metaKey))) {
        e.preventDefault();
        open();
      }
    });
  }
  // --- skins ---------------------------------------------------------------------
  // A skin is a named palette inside the theme. It is applied with a data-skin attribute (see
  // the head script for the pre-paint restore) and remembered per visitor.
  var skinButtons = document.querySelectorAll("[data-skin-set]");
  if (skinButtons.length) {
    var currentSkin = function () {
      return root.getAttribute("data-skin") || root.getAttribute("data-default-skin") || "";
    };
    var markSkin = function () {
      var now = currentSkin();
      skinButtons.forEach(function (b) {
        b.setAttribute("aria-checked", String(b.getAttribute("data-skin-set") === now));
      });
    };
    markSkin();
    skinButtons.forEach(function (b) {
      b.addEventListener("click", function () {
        var name = b.getAttribute("data-skin-set");
        root.setAttribute("data-skin", name);
        try { localStorage.setItem("meteorite-skin", name); } catch (e) {}
        markSkin();
        var menu = b.closest("details");
        if (menu) menu.removeAttribute("open");
      });
    });
    document.addEventListener("click", function (e) {
      document.querySelectorAll("details.skin-menu[open]").forEach(function (d) {
        if (!d.contains(e.target)) d.removeAttribute("open");
      });
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape") {
        document.querySelectorAll("details.skin-menu[open]").forEach(function (d) { d.removeAttribute("open"); });
      }
    });
  }
  // --- print (resume) -----------------------------------------------------------
  document.querySelectorAll("[data-print]").forEach(function (b) {
    b.hidden = false;
    b.addEventListener("click", function () { window.print(); });
  });
  // --- activity graph -----------------------------------------------------------
  // Built from the dates of the site's pages. It ends at the newest date rather than today, so the
  // same content always draws the same graph.
  var graph = document.getElementById("activity-graph");
  var feed = document.getElementById("activity-data");
  if (graph && feed) {
    var dates = [];
    try { dates = JSON.parse(feed.textContent).filter(Boolean); } catch (e) {}
    if (!dates.length) {
      var block = document.getElementById("activity");
      if (block) block.hidden = true;
    } else {
      var DAY = 86400000;
      var toTime = function (d) { var p = d.split("-"); return Date.UTC(+p[0], +p[1] - 1, +p[2]); };
      var counts = {}, newest = 0;
      dates.forEach(function (d) {
        var t = toTime(d);
        if (isNaN(t)) return;
        counts[t] = (counts[t] || 0) + 1;
        if (t > newest) newest = t;
      });
      var endOfWeek = newest + (6 - new Date(newest).getUTCDay()) * DAY;
      var start = endOfWeek - (53 * 7 - 1) * DAY;
      var total = 0, frag = document.createDocumentFragment();
      for (var t = start; t <= endOfWeek; t += DAY) {
        var n = counts[t] || 0;
        total += n;
        var cell = document.createElement("i");
        var level = n === 0 ? 0 : n === 1 ? 1 : n === 2 ? 2 : n === 3 ? 3 : 4;
        if (level) cell.setAttribute("data-l", String(level));
        if (t > newest) cell.style.visibility = "hidden";
        cell.title = (n || "No") + (n === 1 ? " item on " : " items on ") + new Date(t).toISOString().slice(0, 10);
        frag.appendChild(cell);
      }
      graph.appendChild(frag);
      graph.scrollLeft = graph.scrollWidth;
      var label = document.getElementById("activity-total");
      if (label) label.textContent = total + (total === 1 ? " item" : " items") + " in the last year";
    }
  }
})();
