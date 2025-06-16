"use strict";

var version = 'v1.0.0::speki';
console.log('Service Worker: Version', version, 'starting.');

const updateInterval = 4 * 60 * 60 * 1000 // 4 hours in ms
var offlineFundamentals = [
  '/',
  '/512x512.png',
  '/main.css',
];

function checkForUpdates() {
  console.log("Service Worker: Checking for updates")
  self.registration.update();
}

setInterval(checkForUpdates, updateInterval);

self.addEventListener("install", function(event) {
  console.log('Service Worker: Install event in progress.');
  event.waitUntil(
    caches.open(version + 'fundamentals')
      .then(function(cache) {
        console.log('Service Worker: Caching offline fundamentals.');
        return cache.addAll(offlineFundamentals);
      })
      .then(function() {
        console.log('Service Worker: Install completed.');
      })
      .catch(function(error) {
        console.error('Service Worker: Install failed:', error);
      })
  );
  self.skipWaiting(); // Immediatly activate after an update
});

self.addEventListener("fetch", function(event) {
  console.log('Service Worker: Fetch event in progress.');

  if (event.request.method !== 'GET' || event.request.url.includes("status")) {
    console.log('Service Worker: Fetch event ignored.', event.request.method, event.request.url);
    return;
  }

  event.respondWith(
    caches.match(event.request)
      .then(function(cached) {
        var networked = fetch(event.request)
          .then(fetchedFromNetwork, unableToResolve)
          .catch(unableToResolve);

        console.log('Service Worker: Fetch event', cached ? '(cached)' : '(network)', event.request.url);
        return cached || networked;

        function fetchedFromNetwork(response) {
          var cacheCopy = response.clone();
          console.log('Service Worker: Fetch response from network.', event.request.url);
          caches.open(version + 'pages')
            .then(function(cache) {
              cache.put(event.request, cacheCopy);
            })
            .then(function() {
              console.log('Service Worker: Fetch response stored in cache.', event.request.url);
            });
          return response;
        }

        function unableToResolve() {
          console.log('Service Worker: Fetch request failed in both cache and network.');
          return new Response('<h1>Service Unavailable</h1>', {
            status: 503,
            statusText: 'Service Unavailable',
            headers: new Headers({
              'Content-Type': 'text/html'
            })
          });
        }
      })
  );
});

self.addEventListener("activate", function(event) {
  console.log('Service Worker: Activate event in progress.');
  event.waitUntil(
    caches.keys()
      .then(function(keys) {
        return Promise.all(
          keys.filter(function(key) {
            return !key.startsWith(version);
          })
          .map(function(key) {
            return caches.delete(key);
          })
        );
      })
      .then(function() {
        console.log('Service Worker: Activate completed.');
      })
  );
});
