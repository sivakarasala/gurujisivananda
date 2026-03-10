const CACHE_VERSION = 'v1';
const STATIC_CACHE = `gurujisivananda-static-${CACHE_VERSION}`;
const AUDIO_CACHE = 'gurujisivananda-audio';

const PRECACHE_URLS = [
  '/guruji.jpg',
  '/favicon.ico',
  '/manifest.json',
];

// ---- INSTALL: pre-cache static assets ----
// Note: We don't pre-cache /, .js, .wasm, .css because cargo-leptos
// may hash filenames on rebuild. These get cached on first fetch instead.
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(STATIC_CACHE).then((cache) => {
      return cache.addAll(PRECACHE_URLS);
    })
  );
  self.skipWaiting();
});

// ---- ACTIVATE: clean up old static caches ----
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) => {
      return Promise.all(
        keys
          .filter((key) => key.startsWith('gurujisivananda-static-') && key !== STATIC_CACHE)
          .map((key) => caches.delete(key))
      );
    })
  );
  self.clients.claim();
});

// ---- FETCH: routing strategy ----
self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);

  // Only handle same-origin requests
  if (url.origin !== location.origin) return;

  // 1) Audio stream requests: cache-first for saved tracks with Range support
  if (url.pathname.match(/^\/api\/v1\/tracks\/[^/]+\/stream$/)) {
    event.respondWith(
      caches.open(AUDIO_CACHE).then((cache) => {
        return cache.match(url.pathname).then((cached) => {
          if (!cached) {
            return fetch(event.request);
          }

          // Handle Range requests for seeking within cached audio
          const rangeHeader = event.request.headers.get('Range');
          if (!rangeHeader) {
            return cached;
          }

          return cached.arrayBuffer().then((buf) => {
            const bytes = new Uint8Array(buf);
            const total = bytes.length;

            const match = rangeHeader.match(/bytes=(\d+)-(\d*)/);
            if (!match) return cached;

            const start = parseInt(match[1], 10);
            const end = match[2] ? parseInt(match[2], 10) : total - 1;
            const slice = bytes.slice(start, end + 1);

            return new Response(slice, {
              status: 206,
              statusText: 'Partial Content',
              headers: {
                'Content-Type': cached.headers.get('Content-Type') || 'audio/mpeg',
                'Content-Length': slice.length,
                'Content-Range': `bytes ${start}-${end}/${total}`,
                'Accept-Ranges': 'bytes',
              },
            });
          });
        });
      })
    );
    return;
  }

  // 2) Track list API: network-first with cache fallback
  if (url.pathname === '/api/v1/tracks') {
    event.respondWith(
      fetch(event.request)
        .then((response) => {
          if (response.ok && event.request.method === 'GET') {
            const clone = response.clone();
            caches.open(STATIC_CACHE).then((cache) => cache.put(event.request, clone));
          }
          return response;
        })
        .catch(() => caches.match(event.request))
    );
    return;
  }

  // 3) Skip non-GET requests (server functions, etc.)
  if (event.request.method !== 'GET') return;

  // 4) Navigation requests (HTML pages): network-first so SSR always works
  if (event.request.mode === 'navigate') {
    event.respondWith(
      fetch(event.request)
        .then((response) => {
          if (response.ok) {
            const clone = response.clone();
            caches.open(STATIC_CACHE).then((cache) => cache.put(event.request, clone));
          }
          return response;
        })
        .catch(() => caches.match(event.request))
    );
    return;
  }

  // 5) Static assets (JS, WASM, CSS, images): stale-while-revalidate
  event.respondWith(
    caches.match(event.request).then((cached) => {
      const fetchPromise = fetch(event.request).then((response) => {
        if (response.ok) {
          const clone = response.clone();
          caches.open(STATIC_CACHE).then((cache) => cache.put(event.request, clone));
        }
        return response;
      });
      return cached || fetchPromise;
    })
  );
});

// ---- MESSAGE: handle offline audio save/remove from WASM ----
self.addEventListener('message', (event) => {
  if (!event.data) return;

  if (event.data.type === 'SAVE_TRACK_OFFLINE') {
    const trackUrl = event.data.url;
    event.waitUntil(
      caches.open(AUDIO_CACHE).then((cache) => {
        return fetch(trackUrl).then((response) => {
          if (response.ok) {
            // Store using just the pathname as key for consistent matching
            const url = new URL(trackUrl, location.origin);
            return cache.put(url.pathname, response);
          }
          throw new Error('Failed to fetch audio for caching');
        });
      }).then(() => {
        // Notify the client of success
        if (event.source) {
          event.source.postMessage({ type: 'TRACK_SAVED', url: trackUrl, success: true });
        }
      }).catch((err) => {
        if (event.source) {
          event.source.postMessage({ type: 'TRACK_SAVED', url: trackUrl, success: false, error: err.message });
        }
      })
    );
  }

  if (event.data.type === 'REMOVE_TRACK_OFFLINE') {
    const trackUrl = event.data.url;
    event.waitUntil(
      caches.open(AUDIO_CACHE).then((cache) => {
        const url = new URL(trackUrl, location.origin);
        return cache.delete(url.pathname);
      }).then(() => {
        if (event.source) {
          event.source.postMessage({ type: 'TRACK_REMOVED', url: trackUrl, success: true });
        }
      })
    );
  }
});
