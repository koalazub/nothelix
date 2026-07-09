import init, {
  conceal_overlays,
  unicode_completions,
  markdown_to_typst,
} from './wasm/nothelix.js';

const esc = (s) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function applyConceal(text, overlaysJson) {
  let overlays;
  try {
    overlays = JSON.parse(overlaysJson);
  } catch {
    return esc(text);
  }
  const map = new Map(overlays.map((o) => [o.offset, o.replacement]));
  const chars = [...text];
  let out = '';
  for (let i = 0; i < chars.length; i++) {
    out += map.has(i) ? esc(map.get(i)) : esc(chars[i]);
  }
  return out;
}

function wireConceal(root) {
  const input = root.querySelector('[data-nx-in]');
  const out = root.querySelector('[data-nx-out]');
  const render = () => {
    out.innerHTML = applyConceal(input.value, conceal_overlays(input.value));
  };
  input.addEventListener('input', render);
  render();
}

function wireCompletion(root) {
  const input = root.querySelector('[data-nx-in]');
  const list = root.querySelector('[data-nx-list]');
  const render = () => {
    const q = input.value.replace(/^\\/, '');
    let items = [];
    if (q) {
      try {
        items = JSON.parse(unicode_completions(q));
      } catch {
        items = [];
      }
    }
    if (!items.length) {
      list.innerHTML = '<div class="nx-cmp__empty">type after the backslash…</div>';
      return;
    }
    list.innerHTML = items
      .slice(0, 10)
      .map(
        (it) =>
          `<div class="nx-cmp__row"><span class="nx-cmp__glyph">${esc(
            it.char
          )}</span><span class="nx-cmp__name">\\${esc(it.name)}</span></div>`
      )
      .join('');
  };
  input.addEventListener('input', render);
  render();
}

function wireMdTypst(root) {
  const input = root.querySelector('[data-nx-in]');
  const out = root.querySelector('[data-nx-out]');
  const render = () => {
    out.textContent = markdown_to_typst(input.value);
  };
  input.addEventListener('input', render);
  render();
}

function wireMathRender(root) {
  const input = root.querySelector('[data-nx-in]');
  const out = root.querySelector('[data-nx-out]');
  const render = () => {
    if (!window.katex) {
      out.textContent = input.value;
      return;
    }
    try {
      window.katex.render(input.value, out, {
        displayMode: true,
        throwOnError: false,
      });
    } catch {
      out.textContent = input.value;
    }
  };
  input.addEventListener('input', render);
  if (window.katex) render();
  else window.addEventListener('load', render);
}

document.querySelectorAll('[data-nx-mathrender]').forEach(wireMathRender);

init()
  .then(() => {
    document.querySelectorAll('[data-nx-conceal]').forEach(wireConceal);
    document.querySelectorAll('[data-nx-completion]').forEach(wireCompletion);
    document.querySelectorAll('[data-nx-mdtypst]').forEach(wireMdTypst);
    document
      .querySelectorAll('[data-nx-wasm-status]')
      .forEach((el) => el.remove());
  })
  .catch((err) => {
    document.querySelectorAll('[data-nx-wasm-status]').forEach((el) => {
      el.textContent = 'WebAssembly failed to load: ' + err;
    });
  });
