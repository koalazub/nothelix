(function () {
  document.querySelectorAll('[data-nx-sim]').forEach(function (root) {
    var stage = root.querySelector('[data-nx-stage]');
    var railEl = root.querySelector('[data-nx-rail]');
    var dotsEl = root.querySelector('[data-nx-dots]');
    var countEl = root.querySelector('[data-nx-count]');
    var replay = root.querySelector('[data-nx-replay]');
    var reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    var ACCENT = ['helix', 'steel', 'rust', 'julia'];
    var RAIL = ['Helix', 'plugin', 'libnothelix', 'kernel'];

    var frames = [
      { layer: 1, kind: 'buf', badge: '&#9654; :execute-cell',
        cap: 'You put the cursor in a cell and run <b>:execute-cell</b>. The Steel plugin reads just that one cell &mdash; its code and index.' },
      { layer: 2, kind: 'doc', tab: 'input.json',
        body: '{\n  "index": 3,\n  "code": "plot(x, y)"\n}',
        note: 'written into the kernel scratch directory',
        cap: '<b>libnothelix</b> serialises the cell to a JSON file the kernel is watching. The IPC is plain files &mdash; no sockets, no ports.' },
      { layer: 3, kind: 'doc', tab: 'julia kernel',
        body: 'julia> Core.eval(Notebook, :( plot(x, y) ))\n\n# the Notebook module already holds:\n#   x = 1:10      (from cell 1)\n#   y = x .^ 2    (from cell 2)',
        cap: 'The kernel evaluates it inside <b>one long-lived module</b>, so variables from earlier cells are still alive &mdash; exactly like a REPL.' },
      { layer: 3, kind: 'doc', tab: 'output.json',
        body: '{\n  "text":     "",\n  "png_b64":  "iVBORw0KGgo...",\n  "error":    null,\n  "registry": { ... }\n}',
        note: 'text, figures, errors, and a cell-registry snapshot',
        cap: 'Whatever the cell produced is captured and written straight back out as structured JSON &mdash; here, a PNG figure.' },
      { layer: 2, kind: 'doc', tab: 'kitty graphics',
        body: 'ESC _G a=T, f=100, t=d, c=64, r=18 ;\n      <base64 PNG, sent in chunks>\nESC \\\\',
        note: 'the kernel never knew what a terminal was',
        cap: 'Back in Rust, <b>libnothelix</b> turns the PNG into a Kitty graphics escape &mdash; the terminal native image format.' },
      { layer: 0, kind: 'buf', plot: true, badge: 'Out [3]',
        cap: 'The plugin registers it as <b>RawContent</b> and the forked Helix paints it inline, under the cell. The whole loop never left the editor.' }
    ];

    function esc(s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
    function fmt(s) {
      return s.split('\n').map(function (line) {
        var e = esc(line);
        var m = e.match(/^(\s*)(#.*)$/);
        if (m) { return m[1] + '<span class="c">' + m[2] + '</span>'; }
        e = e.replace(/^(julia&gt; )/, '<span class="k">$1</span>');
        e = e.replace(/"(\w+)":/g, '"<span class="k">$1</span>":');
        return e;
      }).join('\n');
    }
    function plotSVG() {
      var W = 264, H = 116, pl = 28, pr = 10, pt = 12, pb = 24, pts = [];
      for (var x = 1; x <= 10; x++) {
        var px = pl + (x - 1) / 9 * (W - pl - pr);
        var py = (H - pb) - (x * x / 100) * ((H - pb) - pt);
        pts.push([px.toFixed(1), py.toFixed(1)]);
      }
      var poly = pts.map(function (p) { return p[0] + ',' + p[1]; }).join(' ');
      var dots = pts.map(function (p) { return '<circle cx="' + p[0] + '" cy="' + p[1] + '" r="2.2" style="fill:var(--nx-helix)"/>'; }).join('');
      return '<svg viewBox="0 0 ' + W + ' ' + H + '" width="' + W + '" height="' + H + '" role="img" aria-label="line plot of y equals x squared rising steeply">'
        + '<line x1="' + pl + '" y1="' + pt + '" x2="' + pl + '" y2="' + (H - pb) + '" style="stroke:var(--nx-line-strong);stroke-width:1"/>'
        + '<line x1="' + pl + '" y1="' + (H - pb) + '" x2="' + (W - pr) + '" y2="' + (H - pb) + '" style="stroke:var(--nx-line-strong);stroke-width:1"/>'
        + '<polyline points="' + poly + '" style="fill:none;stroke:var(--nx-helix);stroke-width:2;stroke-linejoin:round"/>'
        + dots
        + '<text x="' + (W - pr) + '" y="' + (pt + 7) + '" text-anchor="end" style="font-family:var(--nx-mono);font-size:9px;fill:var(--nx-faint)">y = x2</text>'
        + '</svg>';
    }
    function buildBuf(f) {
      var lines = ['# @cell 3 :julia', 'x = 1:10', 'y = x .^ 2', 'plot(x, y)'];
      var rows = lines.map(function (ln, i) {
        var cls = 'nx-buf__line' + (i === 0 ? ' cmt' : '') + (i === 3 ? ' is-run' : '');
        var c = esc(ln);
        if (i === 3) { c = '<span class="nx-buf__caret">&#9656; </span>' + c; }
        return '<div class="' + cls + '">' + c + '</div>';
      }).join('');
      var out = '<div class="nx-buf__out"><span class="nx-buf__badge">' + f.badge + '</span>'
        + (f.plot ? '<div class="nx-buf__plot">' + plotSVG() + '</div>' : '') + '</div>';
      return '<div class="nx-buf"><div class="nx-buf__chrome"><i></i><i></i><i></i><span class="nx-buf__name">maths.jl</span></div>'
        + rows + out + '</div>';
    }
    function buildDoc(f) {
      var note = f.note ? '<div class="nx-doc__note">&#47;&#47; ' + esc(f.note) + '</div>' : '';
      return '<div class="nx-doc"><span class="nx-doc__tab">' + esc(f.tab) + '</span>'
        + '<pre class="nx-doc__body">' + fmt(f.body) + '</pre>' + note + '</div>';
    }

    railEl.innerHTML = RAIL.map(function (name, i) {
      return '<button type="button" class="nx-sim__seg" data-seg="' + i + '" style="--ac:var(--nx-' + ACCENT[i] + ')">' + name + '</button>';
    }).join('');
    stage.innerHTML = frames.map(function (f) {
      var inner = f.kind === 'buf' ? buildBuf(f) : buildDoc(f);
      return '<div class="nx-sim__frame" style="--ac:var(--nx-' + ACCENT[f.layer] + ')">' + inner
        + '<div class="nx-sim__cap">' + f.cap + '</div></div>';
    }).join('');
    dotsEl.innerHTML = frames.map(function (f, i) {
      return '<button class="nx-sim__pip" data-pip="' + i + '" type="button" aria-label="Step ' + (i + 1) + '"></button>';
    }).join('');

    var frameEls = Array.prototype.slice.call(stage.querySelectorAll('.nx-sim__frame'));
    var segEls = Array.prototype.slice.call(railEl.querySelectorAll('.nx-sim__seg'));
    var pipEls = Array.prototype.slice.call(dotsEl.querySelectorAll('.nx-sim__pip'));
    var i = 0, timer = null;

    function show(n) {
      i = n;
      frameEls.forEach(function (el, k) { el.classList.toggle('is-on', k === n); });
      pipEls.forEach(function (el, k) { el.classList.toggle('is-on', k === n); });
      segEls.forEach(function (el, k) { el.classList.toggle('is-on', k === frames[n].layer); });
      countEl.textContent = (n + 1) + ' / ' + frames.length;
    }
    function stopAuto() { if (timer) { clearTimeout(timer); timer = null; } }
    function tick() {
      stopAuto();
      if (i >= frames.length - 1) { return; }
      timer = setTimeout(function () { show(i + 1); tick(); }, reduce ? 2600 : 3400);
    }
    function autoFrom(n) { show(n); tick(); }

    segEls.forEach(function (seg) {
      seg.addEventListener('click', function () {
        stopAuto();
        var layer = +seg.getAttribute('data-seg');
        var group = [];
        frames.forEach(function (f, k) { if (f.layer === layer) { group.push(k); } });
        if (!group.length) { return; }
        var pos = group.indexOf(i);
        show(pos === -1 ? group[0] : group[(pos + 1) % group.length]);
      });
    });
    pipEls.forEach(function (p, k) { p.addEventListener('click', function () { stopAuto(); show(k); }); });
    replay.addEventListener('click', function () { autoFrom(0); });

    show(0);
    if ('IntersectionObserver' in window) {
      var seen = false;
      var io = new IntersectionObserver(function (entries) {
        entries.forEach(function (e) { if (e.isIntersecting && !seen) { seen = true; io.disconnect(); autoFrom(0); } });
      }, { threshold: 0.4 });
      io.observe(root);
    }
  });
})();

(function () {
  document.querySelectorAll('[data-nx-err]').forEach(function (root) {
    var tabs = root.querySelectorAll('[data-nx-tab]');
    var panes = root.querySelectorAll('[data-nx-pane]');
    tabs.forEach(function (tab) {
      tab.addEventListener('click', function () {
        var idx = tab.getAttribute('data-nx-tab');
        tabs.forEach(function (t) { t.setAttribute('aria-selected', t === tab ? 'true' : 'false'); });
        panes.forEach(function (p) { p.classList.toggle('is-on', p.getAttribute('data-nx-pane') === idx); });
      });
    });
  });
})();
