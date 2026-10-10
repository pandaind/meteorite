/* Meteorite default theme: colour-mode toggle and copy buttons. Progressive enhancement:
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
})();
