(policy => {
  'use strict';
  const stateKey = '__slimYouTubeProtection';
  globalThis[stateKey]?.stop();
  const youtubeHost = host => host === 'youtube.com' || host.endsWith('.youtube.com') ||
    host === 'youtube-nocookie.com' || host.endsWith('.youtube-nocookie.com');
  if (!policy.enabled || !/^https?:$/.test(location.protocol) || !youtubeHost(location.hostname) ||
      policy.exceptions.includes(location.origin)) return;

  // Network URL rules cannot separate ads from content in the same player JSON.
  // See https://github.com/uBlockOrigin/uAssets/blob/master/filters/filters.txt
  const adKeys = new Set(['playerAds', 'adPlacements', 'adSlots', 'adBreakHeartbeatParams']);
  const adMarker = /"(?:playerAds|adPlacements|adSlots|adBreakHeartbeatParams)"\s*:/;
  const parse = JSON.parse;
  const stringify = JSON.stringify;
  const restores = [];
  let active = true;
  let pending = 0;
  let interval = 0;
  let observer;
  function sanitize(value) {
    if (!active || value === null || typeof value !== 'object') return value;
    const seen = new WeakSet();
    let remaining = 30000;
    function visit(node, depth) {
      if (!node || typeof node !== 'object' || depth > 40 || --remaining < 0 || seen.has(node)) return;
      seen.add(node);
      for (const key of Object.keys(node)) {
        try {
          if (adKeys.has(key)) { delete node[key]; continue; }
          if (key === 'player_response' && typeof node[key] === 'string') {
            node[key] = sanitizeText(node[key]);
          } else visit(node[key], depth + 1);
        } catch { /* A frozen or accessor-backed page object must not break playback. */ }
      }
    }
    visit(value, 0);
    return value;
  }
  function sanitizeText(text) {
    if (!active || typeof text !== 'string' || !adMarker.test(text)) return text;
    try { return stringify(sanitize(parse(text))); } catch { return text; }
  }
  function playerEndpoint(input) {
    try {
      const url = new URL(typeof input === 'string' ? input : input?.url ?? String(input), location.href);
      return youtubeHost(url.hostname) && /^https?:$/.test(url.protocol) &&
        (/^\/youtubei\/v\d+\/(?:player|next|browse|search|get_watch|reel\/reel_watch_sequence)\/?$/.test(url.pathname) ||
          /^\/(?:watch|playlist)$/.test(url.pathname));
    } catch { return false; }
  }
  function patch(target, key, descriptor) {
    const before = Object.getOwnPropertyDescriptor(target, key);
    if (before && !before.configurable) return;
    try {
      Object.defineProperty(target, key, { configurable: true, ...descriptor });
      restores.push(() => {
        const current = Object.getOwnPropertyDescriptor(target, key);
        if (current?.value !== descriptor.value || current?.get !== descriptor.get || current?.set !== descriptor.set) return;
        if (before) Object.defineProperty(target, key, before);
        else delete target[key];
      });
    } catch { /* Another script may have locked a built-in. */ }
  }
  for (const key of ['ytInitialPlayerResponse', 'ytInitialData', 'playerResponse', 'ytplayer']) {
    const before = Object.getOwnPropertyDescriptor(globalThis, key);
    if (before && (!before.configurable || before.get || before.set || !before.writable)) {
      try { sanitize(globalThis[key]); } catch {}
      continue;
    }
    let current = sanitize(before?.value);
    const get = () => current;
    const set = value => { current = sanitize(value); };
    try {
      Object.defineProperty(globalThis, key, { configurable: true, enumerable: before?.enumerable ?? true, get, set });
      restores.push(() => {
        if (Object.getOwnPropertyDescriptor(globalThis, key)?.get !== get) return;
        if (before || current !== undefined) Object.defineProperty(globalThis, key, {
          configurable: true, enumerable: before?.enumerable ?? true, writable: true, value: current,
        });
        else delete globalThis[key];
      });
    } catch {}
  }
  function wrapResponse(response) {
    return new Proxy(response, {
      get(target, key) {
        if (key === 'json') return async () => sanitize(await target.json());
        if (key === 'text') return async () => sanitizeText(await target.text());
        if (key === 'clone') return () => wrapResponse(target.clone());
        const value = Reflect.get(target, key, target);
        return typeof value === 'function' ? value.bind(target) : value;
      },
    });
  }
  if (typeof globalThis.fetch === 'function') {
    const original = globalThis.fetch;
    patch(globalThis, 'fetch', { writable: true, value: function (...args) {
      const promise = Reflect.apply(original, this, args);
      return active && playerEndpoint(args[0]) ? promise.then(wrapResponse) : promise;
    } });
  }
  const xhr = globalThis.XMLHttpRequest?.prototype;
  if (xhr) {
    const requests = new WeakMap();
    const open = xhr.open;
    patch(xhr, 'open', { writable: true, value: function (method, url, ...args) {
      requests.set(this, playerEndpoint(url));
      return Reflect.apply(open, this, [method, url, ...args]);
    } });
    for (const key of ['responseText', 'response']) {
      const descriptor = Object.getOwnPropertyDescriptor(xhr, key);
      if (!descriptor?.get) continue;
      patch(xhr, key, { enumerable: descriptor.enumerable, get: function () {
        const value = Reflect.apply(descriptor.get, this, []);
        if (!active || !requests.get(this)) return value;
        if (key === 'response' && this.responseType === 'json') return sanitize(value);
        return sanitizeText(value);
      } });
    }
  }
  const styleId = 'slim-youtube-adblock-style';
  function update() {
    pending = 0;
    if (!active) return;
    if (!document.getElementById(styleId) && document.documentElement) {
      const style = document.createElement('style');
      style.id = styleId;
      // Keep skip controls and the video visible so blocked requests cannot leave a black player.
      style.textContent = 'ytd-ad-slot-renderer,ytd-display-ad-renderer,ytd-promoted-sparkles-web-renderer,' +
        'ytd-promoted-video-renderer,ytd-action-companion-ad-renderer,ytd-in-feed-ad-layout-renderer,' +
        'ytd-banner-promo-renderer,ytd-engagement-panel-section-list-renderer[target-id="engagement-panel-ads"],' +
        '#player-ads,#masthead-ad,.ytp-ad-overlay-container{display:none!important}';
      (document.head || document.documentElement).append(style);
    }
    const player = document.querySelector('#movie_player, .html5-video-player');
    if (!player || !player.classList.contains('ad-showing')) {
      clearInterval(interval);
      interval = 0;
      return;
    }
    if (!interval) interval = setInterval(update, 250);
    const skip = player.querySelector('.ytp-skip-ad-button, .ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-ad-skip-button-container button');
    if (skip && !skip.disabled && skip.getClientRects().length) { skip.click(); return; }
    const badge = player.querySelector('.ytp-ad-preview-container, .ytp-ad-text, .ytp-ad-player-overlay');
    if (!badge?.getClientRects().length) return;
    const video = player.querySelector('video');
    const contentLength = Number(globalThis.ytInitialPlayerResponse?.videoDetails?.lengthSeconds);
    // shortcut: only a finite, identified ad segment is seeked; server-stitched ads need maintained new rules.
    if (video && Number.isFinite(video.duration) && video.duration > 0 && video.duration < 600 &&
        video.duration !== contentLength && video.currentTime < video.duration) {
      try { video.currentTime = video.duration; } catch {}
    }
  }
  function schedule() {
    if (active && !pending) pending = setTimeout(update, 80);
  }
  const stop = () => {
    active = false;
    clearTimeout(pending);
    clearInterval(interval);
    observer?.disconnect();
    document.removeEventListener('yt-navigate-finish', schedule);
    document.removeEventListener('DOMContentLoaded', schedule);
    document.getElementById(styleId)?.remove();
    for (const restore of restores.reverse()) { try { restore(); } catch {} }
    if (globalThis[stateKey]?.stop === stop) delete globalThis[stateKey];
  };
  Object.defineProperty(globalThis, stateKey, { value: { stop }, configurable: true });
  observer = new MutationObserver(schedule);
  observer.observe(document, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] });
  document.addEventListener('yt-navigate-finish', schedule);
  document.addEventListener('DOMContentLoaded', schedule);
  update();
})
