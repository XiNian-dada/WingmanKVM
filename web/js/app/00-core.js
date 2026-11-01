const $ = (s, root = document) => root.querySelector(s);
const $$ = (s, root = document) => [...root.querySelectorAll(s)];
const authView = $('#auth-view'),
  app = $('#app'),
  setupForm = $('#setup-form'),
  loginForm = $('#login-form');
const viewport = $('#video-viewport'),
  feed = $('#video-feed'),
  webrtcFeed = $('#webrtc-feed'),
  consoleBox = $('#console');
const fieldTooltip = $('#field-tooltip');

export { $, $$, app, authView, consoleBox, feed, fieldTooltip, loginForm, setupForm, viewport, webrtcFeed };
