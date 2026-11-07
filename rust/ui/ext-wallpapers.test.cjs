const { readFileSync } = require('node:fs');
const { runInNewContext } = require('node:vm');
const assert = require('node:assert/strict');
const source = readFileSync(`${__dirname}/ext-wallpapers.js`, 'utf8');
const media = 'https://cdn.extclub.ru/ugc/pub/92f2a67d19c0a179.webp';
function setup(host = 'ikkiomngodfmfdmeomohmjeobdgpholf', succeeds = false) {
  const calls = [], nodes = [], events = {};
  const pane = { prepend: node => nodes.push(node) };
  const context = {
    location: { protocol: 'chrome-extension:', host, pathname: '/wallpaper.html', href: `chrome-extension://${host}/wallpaper.html` },
    document: {
      documentElement: { lang: 'en' }, querySelector: () => pane,
      createElement: () => ({ children: [], setAttribute() {}, addEventListener() {}, append(...children) { this.children.push(...children); } })
    },
    window: { fetch: async (input, init) => { calls.push({ input, init }); if (!succeeds && String(input).includes('cdn.extclub.ru')) throw TypeError('Failed to fetch'); return { ok: true }; } },
    addEventListener: (type, handler) => { events[type] = handler; },
    URL, AbortController, AbortSignal, DOMException, setTimeout, clearTimeout
  };
  runInNewContext(source, context);
  return { context, calls, nodes, events };
}
(async () => {
  const { context, calls, nodes, events } = setup();
  await context.window.fetch('https://api.extclub.ru/ext/wallpapers');
  assert.equal(calls[0].init.signal, undefined, 'Catalog request must remain unchanged');
  await assert.rejects(context.window.fetch(media), /Failed to fetch/);
  assert.equal(nodes.length, 1);
  assert.match(nodes[0].children[0].textContent, /cdn.extclub.ru/);
  assert.equal(nodes[0].hidden, false);
  events.error({ target: { src: media } });
  assert.equal(nodes.length, 1, 'Repeated failures must share one message');
  const unrelated = setup('another-extension');
  await assert.rejects(unrelated.context.window.fetch(media));
  assert.equal(unrelated.nodes.length, 0, 'Adapter must not modify other extensions');
  const abort = new AbortController(); abort.abort();
  const isolated = setup();
  await assert.rejects(isolated.context.window.fetch(media, { signal: abort.signal }));
  assert.equal(isolated.calls[0].init.signal.aborted, true);
  assert.equal(isolated.nodes.length, 0, 'Caller cancellation must not report a CDN outage');
  const streaming = setup(undefined, true), streamingAbort = new AbortController();
  await streaming.context.window.fetch(media, { signal: streamingAbort.signal });
  streamingAbort.abort();
  assert.equal(streaming.calls[0].init.signal.aborted, true, 'Caller cancellation must still abort the body after headers');
  console.log('EXT media adapter: 5 behavior checks passed');
})().catch(error => { console.error(error); process.exitCode = 1; });
