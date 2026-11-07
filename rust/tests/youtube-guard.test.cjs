// Run: node --test rust/tests/youtube-guard.test.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');
const sourcePath = path.join(__dirname, '../ui/youtube-guard.js');
const source = fs.readFileSync(sourcePath, 'utf8');
const dirty = () => ({
  videoDetails: { videoId: 'content', lengthSeconds: '360' },
  streamingData: { formats: [{ url: 'https://rr1.googlevideo.com/videoplayback?content=1' }] },
  playerAds: [{ ad: true }], adPlacements: [{ ad: true }], adSlots: [{ ad: true }],
  captions: { playerCaptionsTracklistRenderer: { captionTracks: [{ languageCode: 'ru' }] } },
});
function fixture({ url = 'https://www.youtube.com/watch?v=content', body, ad = false, skip = false } = {}) {
  const timers = new Map();
  const listeners = new Map();
  const styles = new Map();
  let nextTimer = 0;
  const video = { duration: 30, currentTime: 0, paused: false, playbackRate: 1, muted: false };
  const button = { disabled: false, clicks: 0, getClientRects: () => [{}], click() { this.clicks++; } };
  const player = {
    classList: { contains: name => ad && name === 'ad-showing' },
    querySelector(selector) {
      if (selector.includes('skip')) return skip ? button : null;
      if (selector.includes('video')) return video;
      return ad ? { getClientRects: () => [{}] } : null;
    },
  };
  const document = {
    documentElement: { append(node) { styles.set(node.id, node); } },
    head: null,
    getElementById: id => styles.get(id) || null,
    createElement: () => ({ id: '', textContent: '', remove() { styles.delete(this.id); } }),
    querySelector: selector => selector.includes('movie_player') || selector.includes('html5-video-player') ? player : null,
    querySelectorAll: () => [],
    addEventListener(name, listener) { listeners.set(name, listener); },
    removeEventListener(name) { listeners.delete(name); },
  };
  class XMLHttpRequest {
    open(method, url) { this.url = url; }
    get responseText() { return this.raw || ''; }
    get response() { return this.responseType === 'json' ? this.json : this.responseText; }
  }
  const fetch = async () => {
    const response = new Response(typeof body === 'string' ? body : JSON.stringify(body ?? dirty()), {
      status: 200, headers: { 'content-type': 'application/json', 'x-fixture': 'preserve' },
    });
    Object.defineProperty(response, 'url', { value: 'https://www.youtube.com/youtubei/v1/player' });
    return response;
  };
  const context = vm.createContext({
    URL, Response, Request, location: new URL(url), document, XMLHttpRequest, fetch,
    MutationObserver: class { observe() {} disconnect() {} },
    setTimeout(fn) { timers.set(++nextTimer, fn); return nextTimer; },
    clearTimeout(id) { timers.delete(id); },
    setInterval(fn) { timers.set(++nextTimer, fn); return nextTimer; },
    clearInterval(id) { timers.delete(id); },
    addEventListener: document.addEventListener.bind(document),
    removeEventListener: document.removeEventListener.bind(document),
  });
  context.window = context;
  const install = (policy = { enabled: true, exceptions: [] }) => vm.runInContext(`(${source})(${JSON.stringify(policy)})`, context);
  return { context, install, timers, listeners, styles, video, button, fetch };
}
function contentSurvives(value) {
  assert.equal(value.playerAds, undefined);
  assert.equal(value.adPlacements, undefined);
  assert.equal(value.adSlots, undefined);
  assert.equal(value.videoDetails.videoId, 'content');
  assert.equal(value.streamingData.formats[0].url, 'https://rr1.googlevideo.com/videoplayback?content=1');
  assert.equal(value.captions.playerCaptionsTracklistRenderer.captionTracks[0].languageCode, 'ru');
}
test('initial and replacement player payloads lose ad instructions without losing media or captions', () => {
  const f = fixture(); f.install();
  f.context.ytInitialPlayerResponse = dirty();
  contentSurvives(f.context.ytInitialPlayerResponse);
  f.context.ytInitialPlayerResponse = dirty();
  contentSurvives(f.context.ytInitialPlayerResponse);
});
test('fetch player JSON, text and cloned responses preserve native metadata and remove ads', async () => {
  const f = fixture(); f.install();
  const response = await f.context.fetch('/youtubei/v1/player?key=fixture');
  assert.equal(response instanceof Response, true);
  assert.equal(response.status, 200);
  assert.equal(response.url, 'https://www.youtube.com/youtubei/v1/player');
  assert.equal(response.headers.get('x-fixture'), 'preserve');
  contentSurvives(JSON.parse(await response.clone().text()));
  contentSurvives(await response.json());
  assert.equal(response.bodyUsed, true);
  await assert.rejects(response.json(), TypeError);
});
test('XHR text and JSON player responses are sanitized', () => {
  const f = fixture(); f.install();
  const xhr = new f.context.XMLHttpRequest(); xhr.open('POST', '/youtubei/v1/player');
  xhr.raw = JSON.stringify(dirty());
  contentSurvives(JSON.parse(xhr.responseText));
  xhr.responseType = 'json'; xhr.json = dirty();
  contentSurvives(xhr.response);
});
test('watch SPA arrays and embedded player_response strings are sanitized', async () => {
  const f = fixture({ body: [{ playerResponse: dirty() }] }); f.install();
  contentSurvives((await (await f.context.fetch('/watch?v=content&pbj=1')).json())[0].playerResponse);
  f.context.ytplayer = { config: { args: { player_response: JSON.stringify(dirty()) } } };
  contentSurvives(JSON.parse(f.context.ytplayer.config.args.player_response));
});
test('malformed bodies and non-player endpoints pass through unchanged', async () => {
  const f = fixture({ body: 'not json' }); f.install();
  assert.equal(await (await f.context.fetch('/youtubei/v1/player')).text(), 'not json');
  const g = fixture(); g.install();
  assert.equal((await (await g.context.fetch('https://example.com/youtubei/v1/player')).json()).adSlots.length, 1);
  assert.equal((await (await g.context.fetch('/api/timedtext')).json()).playerAds.length, 1);
});
test('disabled, excepted and lookalike origins retain original behavior', () => {
  for (const [url, policy] of [
    ['https://www.youtube.com/', { enabled: false, exceptions: [] }],
    ['https://www.youtube.com/', { enabled: true, exceptions: ['https://www.youtube.com'] }],
    ['https://youtube.com.evil.invalid/', { enabled: true, exceptions: [] }],
    ['https://notyoutube.com/', { enabled: true, exceptions: [] }],
  ]) {
    const f = fixture({ url }); f.install(policy); f.context.ytInitialPlayerResponse = dirty();
    assert.equal(f.context.ytInitialPlayerResponse.playerAds.length, 1);
    assert.equal(f.context.fetch, f.fetch);
    assert.equal(f.timers.size, 0);
  }
});
test('exact-origin exception does not disable mobile or nocookie domains', () => {
  for (const url of ['https://m.youtube.com/', 'https://www.youtube-nocookie.com/embed/content']) {
    const f = fixture({ url }); f.install({ enabled: true, exceptions: ['https://www.youtube.com'] });
    f.context.ytInitialPlayerResponse = dirty(); contentSurvives(f.context.ytInitialPlayerResponse);
  }
});
test('normal content keeps playback position, rate, mute and pause state', () => {
  const f = fixture(); f.install();
  for (const fn of [...f.timers.values()]) fn();
  assert.deepEqual(f.video, { duration: 30, currentTime: 0, paused: false, playbackRate: 1, muted: false });
  assert.equal(f.timers.size, 0, 'normal playback must have no polling timer');
});
test('visible skip control is clicked only inside an active ad', () => {
  const f = fixture({ ad: true, skip: true }); f.install();
  assert.equal(f.button.clicks, 1);
  const g = fixture({ skip: true }); g.install(); assert.equal(g.button.clicks, 0);
});
test('finite ad segment can end without changing rate or mute; live ads are not seeked', () => {
  const f = fixture({ ad: true }); f.install(); assert.equal(f.video.currentTime, 30);
  assert.equal(f.video.playbackRate, 1); assert.equal(f.video.muted, false);
  const g = fixture({ ad: true }); g.video.duration = Infinity; g.install(); assert.equal(g.video.currentTime, 0);
});
test('an ad marker cannot seek a video matching the known content duration', () => {
  const f = fixture({ ad: true });
  f.context.ytInitialPlayerResponse = dirty();
  f.video.duration = 360;
  f.install();
  assert.equal(f.video.currentTime, 0);
});
test('Request and URL objects use the same endpoint restriction as URL strings', async () => {
  const f = fixture(); f.install();
  contentSurvives(await (await f.context.fetch(new Request('https://www.youtube.com/youtubei/v1/player'))).json());
  contentSurvives(await (await f.context.fetch(new URL('https://www.youtube.com/youtubei/v1/player'))).json());
});
test('turning protection off restores native hooks and removes timers and styles', async () => {
  const f = fixture(); f.install(); f.context.ytInitialPlayerResponse = dirty();
  f.install({ enabled: false, exceptions: [] });
  assert.equal(f.context.fetch, f.fetch);
  assert.equal(f.timers.size, 0); assert.equal(f.styles.size, 0);
  assert.equal(f.context.ytInitialPlayerResponse.videoDetails.videoId, 'content');
  f.context.ytInitialPlayerResponse = dirty(); assert.equal(f.context.ytInitialPlayerResponse.adSlots.length, 1);
  f.install(); contentSurvives(await (await f.context.fetch('/youtubei/v1/player')).json());
});
