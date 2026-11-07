(async () => {
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  const until = async (check, timeout) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      const value = await check();
      if (value) return value;
      await delay(100);
    }
    return null;
  };
  const result = { url: location.href, accountRequired: false };
  try {
    const response = await fetch('https://api.extclub.ru/ext/wallpapers/u/feed?sort=top&off=0', { signal: AbortSignal.timeout(12000) });
    result.apiStatus = response.status;
    const feed = await response.json();
    const item = feed.wallpapers.filter(x => x.kind === 'image' && x.bytes > 0 && x.bytes <= 1024 * 1024).sort((a, b) => a.bytes - b.bytes)[0];
    if (!item) throw Error('No small public image available');
    result.item = { id: item.id, url: item.url, thumb: item.thumb, bytes: item.bytes };
    result.galleryCards = document.querySelectorAll('.g-card').length;
    try {
      const media = await fetch(item.url, { signal: AbortSignal.timeout(10000) });
      result.mediaStatus = media.status;
      const blob = await media.blob();
      const image = await createImageBitmap(blob);
      result.download = { bytes: blob.size, width: image.width, height: image.height };
      image.close();
    } catch (error) {
      result.mediaError = String(error);
    }
    const card = await until(() => document.querySelector(`.g-card[data-src="u:${item.id}"]`), 5000);
    if (card) {
      card.click();
      const meta = await until(async () => {
        const local = await chrome.storage.local.get(['extWpMeta', 'extWpLib']);
        return local.extWpMeta?.src === `u:${item.id}` ? local : null;
      }, 18000);
      result.applied = !!meta;
      if (meta) {
        const blob = await ExtWpDb.get(meta.extWpMeta.libId);
        result.storage = { meta: meta.extWpMeta, libraryCount: meta.extWpLib.length, indexedDbBytes: blob?.size || 0 };
        const frame = document.createElement('iframe');
        frame.src = chrome.runtime.getURL('wp-frame.html');
        document.body.append(frame);
        const rendered = await until(() => {
          const media = frame.contentDocument?.querySelector('#m');
          return media && (media.naturalWidth || media.videoWidth) ? media : null;
        }, 5000);
        result.frame = rendered ? { tag: rendered.tagName, width: rendered.naturalWidth || rendered.videoWidth, height: rendered.naturalHeight || rendered.videoHeight } : null;
        frame.remove();
      }
      result.message = document.getElementById('msg')?.textContent;
    } else {
      result.applied = false;
      result.cardMissing = true;
    }
  } catch (error) {
    result.error = String(error);
  }
  result.mediaErrors = globalThis.__mediaErrors || [];
  result.galleryCards = document.querySelectorAll('.g-card').length;
  const status = document.getElementById('slim-wallpaper-status');
  result.serverStatus = status ? { text: status.textContent, visible: !status.hidden && !status.closest('[hidden]') } : null;
  return result;
})()
