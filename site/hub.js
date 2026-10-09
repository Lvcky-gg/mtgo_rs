'use strict';
(() => {
  const repo = 'https://github.com/Lvcky-gg/mtgo_rs';
  const cards = [...document.querySelectorAll('.resource-card')];
  const filters = [...document.querySelectorAll('[data-filter]')];
  const search = document.querySelector('#resource-search');
  let category = 'all';
  function updateResources() {
    const query = search.value.trim().toLocaleLowerCase();
    let count = 0;
    for (const card of cards) {
      const visible = (category === 'all' || card.dataset.category === category)
        && card.textContent.toLocaleLowerCase().includes(query);
      card.hidden = !visible;
      if (visible) count++;
    }
    document.querySelector('#empty-state').hidden = count !== 0;
    document.querySelector('#resource-status').textContent = `${count} resource${count === 1 ? '' : 's'} shown.`;
    for (const button of filters) {
      const active = button.dataset.filter === category;
      button.classList.toggle('active', active);
      button.setAttribute('aria-pressed', String(active));
    }
  }
  filters.forEach(button => button.addEventListener('click', () => {
    category = button.dataset.filter;
    updateResources();
  }));
  search.addEventListener('input', updateResources);
  document.querySelector('#reset-search').addEventListener('click', () => {
    search.value = '';
    category = 'all';
    updateResources();
    search.focus();
  });
  updateResources();

  // Only highlight architectures we can confidently identify. macOS users
  // choose Apple Silicon or Intel themselves; user agents cannot distinguish them.
  const platform = navigator.userAgent;
  const recommended = /Windows/.test(platform) && !/ARM|aarch64/i.test(platform) ? 'windows'
    : /Linux.*x86_64|X11.*x86_64/.test(platform) && !/Android/.test(platform) ? 'linux' : null;
  if (recommended) {
    const card = document.querySelector(`[data-platform="${recommended}"]`);
    card.classList.add('recommended');
    const label = document.createElement('span');
    label.className = 'platform-recommendation';
    label.textContent = 'FOR YOUR DEVICE';
    card.append(label);
  }

  // Validate GitHub URLs before promoting remote metadata into download links.
  function releaseURL(value, asset = false) {
    try {
      const url = new URL(value);
      const prefix = asset ? '/Lvcky-gg/mtgo_rs/releases/download/' : '/Lvcky-gg/mtgo_rs/releases/tag/';
      return url.origin === 'https://github.com' && url.pathname.startsWith(prefix)
        && !url.username && !url.password ? url.href : null;
    } catch { return null; }
  }
  async function loadRelease() {
    const status = document.querySelector('#release-status');
    try {
      const response = await fetch('https://api.github.com/repos/Lvcky-gg/mtgo_rs/releases/latest', {
        headers: { Accept: 'application/vnd.github+json' }, signal: AbortSignal.timeout(8000)
      });
      if (!response.ok) throw new Error(`GitHub status ${response.status}`);
      const release = await response.json();
      const pageURL = releaseURL(release.html_url);
      if (release.draft || release.prerelease || !pageURL || !Array.isArray(release.assets)
          || typeof release.tag_name !== 'string') throw new Error('Invalid release');
      document.querySelector('#release-badge').textContent = `${release.tag_name} · Latest release`;
      document.querySelector('#release-notes').href = pageURL;
      let available = 0;
      for (const link of document.querySelectorAll('[data-asset-suffix]')) {
        const suffix = link.dataset.assetSuffix;
        const asset = release.assets.find(item => typeof item.name === 'string'
          && item.name.startsWith('mtgo-rs-') && item.name.endsWith(`-${suffix}`)
          && releaseURL(item.browser_download_url, true));
        link.href = asset ? releaseURL(asset.browser_download_url, true) : pageURL;
        link.querySelector('span').textContent = asset ? 'Download' : 'View release';
        if (asset) {
          available++;
          const size = Number(asset.size);
          if (Number.isFinite(size) && size > 0) {
            link.querySelector('span').textContent = `Download · ${Math.round(size / 1048576)} MB`;
          }
        }
      }
      const checksum = release.assets.find(item => item.name === 'SHA256SUMS.txt'
        && releaseURL(item.browser_download_url, true));
      document.querySelector('#checksums').href = checksum ? releaseURL(checksum.browser_download_url, true) : pageURL;
      status.textContent = available === 4 ? 'Latest published release. Downloads come directly from GitHub.'
        : 'Some platform packages are unavailable in this release. View the release for available files.';
    } catch {
      // Static anchors remain useful with no network, no JavaScript or API limits.
      status.textContent = 'View the latest release on GitHub to choose your platform download.';
      document.querySelector('#release-notes').href = `${repo}/releases/latest`;
    }
  }
  loadRelease();
})();
