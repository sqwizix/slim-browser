// Isolated Chromium CDP fixture. --live additionally observes anonymous YouTube.
// Set BROWSER_BINARY to test a different Chromium/Edge build.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const sourcePath = path.join(__dirname, '../ui/youtube-guard.js');
const reportPath = path.join(__dirname, process.argv.includes('--live')
  ? '../target/youtube-guard-browser-live-report.json' : '../target/youtube-guard-browser-report.json');
const payload = {
  playerAds: [{ ad: true }], adPlacements: [{ ad: true }], adSlots: [{ ad: true }],
  videoDetails: { videoId: 'content', lengthSeconds: '360' },
  streamingData: { formats: [{ url: 'https://rr1.googlevideo.com/videoplayback?content=1' }] },
};
const html = `<!doctype html><html><body>
<div id="movie_player"><video></video><button class="ytp-skip-ad-button">Skip</button><span class="ytp-ad-text">Ad</span></div>
<ytd-ad-slot-renderer>Ad slot</ytd-ad-slot-renderer><div id="content">Normal page</div>
<script>
var ytInitialPlayerResponse = ${JSON.stringify(payload)};
window.__inlineClean = !ytInitialPlayerResponse.playerAds && !!ytInitialPlayerResponse.streamingData;
window.__skipClicks = 0;
document.querySelector('button').onclick = () => __skipClicks++;
window.__fixture = (async () => {
  const response = await fetch('/youtubei/v1/player?fixture=1');
  const text = JSON.parse(await response.clone().text());
  const json = await response.json();
  const xhr = await new Promise((resolve,reject) => {
    const request = new XMLHttpRequest(); request.open('POST','/youtubei/v1/player?xhr=1');
    request.responseType = 'json'; request.onload = () => resolve(request.response); request.onerror = reject; request.send('{}');
  });
  const xhrText = await new Promise((resolve,reject) => {
    const request = new XMLHttpRequest(); request.open('POST','/youtubei/v1/player?xhrtext=1');
    request.onload = () => resolve(JSON.parse(request.responseText)); request.onerror = reject; request.send('{}');
  });
  return {inline: __inlineClean, json, text, xhr, xhrText, nativeResponse:response instanceof Response,
    url:response.url,status:response.status,bodyUsed:response.bodyUsed};
})();
</script></body></html>`;
class CDP {
  constructor(socket) {
    this.socket = socket; this.next = 0; this.pending = new Map(); this.listeners = new Map();
    socket.addEventListener('message', event => {
      const value = JSON.parse(event.data);
      if (value.id) {
        const pending = this.pending.get(value.id);
        if (!pending) return;
        this.pending.delete(value.id); clearTimeout(pending.timeout);
        value.error ? pending.reject(new Error(JSON.stringify(value.error))) : pending.resolve(value.result);
      } else for (const listener of this.listeners.get(value.method) || []) listener(value.params);
    });
  }
  on(event, listener) { this.listeners.set(event, [...this.listeners.get(event) || [], listener]); }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      const timeout = setTimeout(() => { this.pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 45000);
      this.pending.set(id, { resolve, reject, timeout });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
  async evaluate(expression, awaitPromise = true) {
    const result = await this.send('Runtime.evaluate', { expression, awaitPromise, returnByValue: true, timeout: 40000 });
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
}
async function main() {
  const source = await fs.readFile(sourcePath, 'utf8');
  const script = policy => `(${source})(${JSON.stringify(policy)})`;
  const enabled = { enabled: true, exceptions: [] };
  const profile = await fs.mkdtemp(path.join(os.tmpdir(), 'slim-youtube-test-'));
  const binary = process.env.BROWSER_BINARY || 'C:/Program Files/Google/Chrome/Application/chrome.exe';
  const browser = spawn(binary, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`,
    '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--autoplay-policy=no-user-gesture-required', 'about:blank'],
    { windowsHide: true, stdio: 'ignore' });
  let launchError;
  browser.on('error', error => { launchError = error; });
  const report = { browser: binary, fixture: {}, live: null };
  let cdp;
  try {
    let port;
    for (let attempt = 0; attempt < 80; attempt++) {
      if (launchError) throw launchError;
      try { port = (await fs.readFile(path.join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0]; break; } catch {}
      await delay(200);
    }
    assert.ok(port, 'fresh browser must start a CDP endpoint');
    const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const socket = new WebSocket(pages.find(page => page.type === 'page').webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
    cdp = new CDP(socket);
    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');
    await cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: 'window.__nativeFetch=fetch;window.__errors=[];addEventListener("error",e=>__errors.push(e.message));' });
    const registration = await cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: script(enabled), runImmediately: true });
    let fixtures = true;
    const transportErrors = [];
    cdp.on('Fetch.requestPaused', event => {
      if (!fixtures) return;
      const url = new URL(event.request.url);
      const document = event.resourceType === 'Document';
      const body = document ? html : JSON.stringify(payload);
      cdp.send('Fetch.fulfillRequest', { requestId: event.requestId, responseCode: 200,
        responseHeaders: [{ name: 'Content-Type', value: document ? 'text/html; charset=utf-8' : 'application/json' }],
        body: Buffer.from(body).toString('base64') }).catch(error => transportErrors.push(String(error)));
    });
    await cdp.send('Fetch.enable', { patterns: [{ urlPattern: '*' }] });
    async function navigate(url) {
      const result = await cdp.send('Page.navigate', { url });
      assert.equal(result.errorText, undefined);
      for (let attempt = 0; attempt < 100; attempt++) {
        if (await cdp.evaluate('!!window.__fixture', false)) return;
        await delay(50);
      }
      throw new Error('fixture did not initialize');
    }
    await navigate('https://www.youtube.com/watch?v=content');
    const result = await cdp.evaluate('window.__fixture');
    assert.equal(result.inline, true, 'main-world injection must precede inline player assignment');
    for (const kind of ['json', 'text', 'xhr', 'xhrText']) {
      assert.equal(result[kind].playerAds, undefined, `${kind} playerAds`);
      assert.equal(result[kind].adSlots, undefined, `${kind} adSlots`);
      assert.equal(result[kind].adPlacements, undefined, `${kind} adPlacements`);
      assert.equal(result[kind].videoDetails.videoId, 'content');
      assert.equal(result[kind].streamingData.formats[0].url, 'https://rr1.googlevideo.com/videoplayback?content=1');
    }
    assert.equal(result.nativeResponse, true); assert.equal(result.bodyUsed, true); assert.equal(result.status, 200);
    assert.match(result.url, /^https:\/\/www\.youtube\.com\/youtubei\/v1\/player/);
    await delay(150);
    assert.equal(await cdp.evaluate('getComputedStyle(document.querySelector("ytd-ad-slot-renderer")).display'), 'none');
    const noAd = await cdp.evaluate('({time:document.querySelector("video").currentTime,rate:document.querySelector("video").playbackRate,clicks:__skipClicks})');
    assert.deepEqual(noAd, { time: 0, rate: 1, clicks: 0 });
    await cdp.evaluate('document.querySelector("#movie_player").classList.add("ad-showing")');
    await delay(200);
    assert.ok(await cdp.evaluate('__skipClicks > 0'));
    await cdp.evaluate(script({ enabled: true, exceptions: ['https://www.youtube.com'] }));
    const disabled = await cdp.evaluate(`(() => {ytInitialPlayerResponse=${JSON.stringify(payload)};return {native:fetch===__nativeFetch,ads:ytInitialPlayerResponse.playerAds.length,style:!!document.getElementById('slim-youtube-adblock-style'),state:!!window.__slimYouTubeProtection}})()`);
    assert.deepEqual(disabled, { native: true, ads: 1, style: false, state: false });
    await cdp.evaluate('history.pushState({}, "", "/watch?v=second");document.dispatchEvent(new Event("yt-navigate-finish"))');
    assert.equal(await cdp.evaluate('fetch===__nativeFetch'), true);
    await cdp.evaluate('document.querySelector("#movie_player").classList.remove("ad-showing")');
    await cdp.evaluate(script(enabled));
    assert.equal(await cdp.evaluate(`(ytInitialPlayerResponse=${JSON.stringify(payload)},!ytInitialPlayerResponse.playerAds)`), true);
    assert.deepEqual(await cdp.evaluate('__errors'), []);
    assert.deepEqual(transportErrors, []);
    report.fixture = { passed: true, inlineBeforePageScript: true, responsePaths: ['fetch.json','fetch.text','clone','xhr.json','xhr.text'],
      normalVideoUntouched: true, visibleSkip: true, originExceptionCleanup: true, reenableAfterSpaNavigation: true };
    console.log('PASS: native Chromium fixture, inline/fetch/XHR, video, toggle, exception and SPA');

    if (process.argv.includes('--live')) {
      fixtures = false;
      await cdp.send('Fetch.disable');
      await cdp.send('Page.removeScriptToEvaluateOnNewDocument', { identifier: registration.identifier });
      await cdp.send('Page.addScriptToEvaluateOnNewDocument', { source: script(enabled), runImmediately: true });
      const requests = [];
      cdp.on('Network.responseReceived', event => { if (/youtubei\/v\d+\/player|videoplayback/.test(event.response.url)) requests.push({ url: event.response.url.split('?')[0], status: event.response.status }); });
      await cdp.send('Network.enable');
      const navigation = await cdp.send('Page.navigate', { url: 'https://www.youtube.com/watch?v=jNQXAC9IVRw' });
      const snapshots = [];
      for (let attempt = 0; attempt < 4; attempt++) {
        await delay(5000);
        snapshots.push(await cdp.evaluate(`(() => {const v=document.querySelector('video');v?.play().catch(()=>{});return {url:location.href,title:document.title,guard:!!window.__slimYouTubeProtection,playability:window.ytInitialPlayerResponse?.playabilityStatus,video:v?{time:v.currentTime,readyState:v.readyState,paused:v.paused,duration:Number.isFinite(v.duration)?v.duration:null,error:v.error?.code}:null,ad:!!document.querySelector('.html5-video-player.ad-showing'),text:document.body?.innerText.slice(0,1100)}})()`));
      }
      report.live = { anonymous: true, navigation, snapshots, requests,
        contentProgressed: snapshots.some((s,i) => i>0 && s.video?.time > (snapshots[i-1].video?.time ?? Infinity) && !s.ad),
        limitation: 'A short anonymous observation does not prove ad blocking for every account, region or server experiment.' };
      console.log(JSON.stringify({ liveContentProgressed: report.live.contentProgressed, lastSnapshot: snapshots.at(-1) }, null, 2));
    }
    await fs.mkdir(path.dirname(reportPath), { recursive: true });
    await fs.writeFile(reportPath, JSON.stringify(report, null, 2));
    console.log(`Report: ${reportPath}`);
  } finally {
    if (cdp) { try { await cdp.send('Browser.close'); } catch {} cdp.socket.close(); }
    if (browser.exitCode === null) {
      await Promise.race([new Promise(resolve => browser.once('exit', resolve)), delay(2000)]);
      if (browser.exitCode === null) browser.kill();
    }
    const resolvedProfile = path.resolve(profile);
    assert.equal(path.dirname(resolvedProfile), path.resolve(os.tmpdir()));
    assert.ok(path.basename(resolvedProfile).startsWith('slim-youtube-test-'));
    await fs.rm(resolvedProfile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }).catch(() => {});
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
