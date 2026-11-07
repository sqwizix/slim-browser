// EXT reports download failures in its hidden "My wallpaper" pane. Keep the feed informed too.
(() => {
  if (location.protocol !== 'chrome-extension:' || location.host !== 'ikkiomngodfmfdmeomohmjeobdgpholf' || !location.pathname.endsWith('/wallpaper.html')) return;
  const nativeFetch = window.fetch.bind(window);
  let status;
  const isMedia = value => {
    try {
      const url = new URL(value, location.href);
      return url.protocol === 'https:' && url.hostname === 'cdn.extclub.ru' && /^\/(wp\/|ugc\/(pub|thumb|pending)\/)/.test(url.pathname);
    } catch { return false; }
  };
  const unavailable = () => {
    if (!status) {
      const pane = document.querySelector('[data-pane="feed"]');
      if (!pane) return;
      status = document.createElement('div');
      status.id = 'slim-wallpaper-status';
      status.className = 'notice';
      status.setAttribute('role', 'status');
      const text = document.createElement('p');
      text.className = 'msg err';
      text.textContent = document.documentElement.lang.startsWith('ru')
        ? 'Не удалось загрузить медиа с cdn.extclub.ru. Каталог доступен, но сервер обоев не отвечает. Повторите попытку позже или загрузите свой файл во вкладке «Мои обои».'
        : 'Media from cdn.extclub.ru could not load. The catalog is available, but the wallpaper server is not responding. Try again later or import a file in My wallpaper.';
      const retry = document.createElement('button');
      retry.type = 'button';
      retry.className = 'btn sm';
      retry.textContent = document.documentElement.lang.startsWith('ru') ? 'Повторить' : 'Retry';
      retry.addEventListener('click', () => location.reload());
      status.append(text, retry);
      pane.prepend(status);
    }
    status.hidden = false;
  };
  window.fetch = async (input, init = {}) => {
    const url = typeof input === 'string' || input instanceof URL ? String(input) : input?.url;
    if (!isMedia(url) || (init.method || input?.method || 'GET').toUpperCase() !== 'GET') return nativeFetch(input, init);
    const controller = new AbortController();
    const callerSignal = init.signal || input?.signal;
    const signal = callerSignal ? AbortSignal.any([callerSignal, controller.signal]) : controller.signal;
    // Only bound the connection/headers wait: large videos keep their original streaming behavior.
    const timer = setTimeout(() => controller.abort(new DOMException('EXT media connection timed out', 'TimeoutError')), 15000);
    try {
      const response = await nativeFetch(input, { ...init, signal });
      if (!response.ok) unavailable();
      else if (status) status.hidden = true;
      return response;
    } catch (error) {
      if (!callerSignal?.aborted) unavailable();
      throw error;
    } finally {
      clearTimeout(timer);
    }
  };
  addEventListener('error', event => {
    if (isMedia(event.target?.currentSrc || event.target?.src)) unavailable();
  }, true);
})()
