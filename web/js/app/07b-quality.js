import { shared } from './state.js';
import { $, feed, webrtcFeed } from './00-core.js';
import { getSetting, setSetting } from './00-settings.js';
import { shouldVideoRun } from './07-video.js';
import {
  adaptiveBitrateText, adaptiveBitrateTick, adaptiveBitrateWantsStats,
} from './07c-adaptive-bitrate.js';

// Connection quality overlay. `connectionQuality` always holds the latest
// computed numbers (null = unknown). Sampling runs while the overlay is shown
// or while adaptive bitrate (#82, 07c) needs WebRTC stats; the overlay
// itself is only visible when the user turned it on.
const connectionQuality = {
  transport: 'none',
  updatedAt: 0,
  rttMs: null,
  bitrateKbps: null,
  packetLossPct: null,
  fps: null,
  fpsSource: null, // 'decoder' | 'browser' | 'server'
  jitterBufferMs: null,
  width: null,
  height: null,
};
const QUALITY_INTERVAL_MS = 1000;
// Firefox fires load for every multipart part, Chrome only for the first, so
// a few events prove that this browser's count is a real frame count.
const MJPEG_MIN_LOADS_TO_TRUST = 3;
let qualityEnabled = getSetting('quality_overlay', false) === true,
    qualityTimer = 0,
    qualityRun = 0,
    qualityTransport = null,
    qualityBaseline = null;
const qualityRows = {};
function setQualityRow(key, label, value) {
  const row = qualityRows[key],
    shown = label !== null;
  if (row.shown !== shown) {
    row.shown = shown;
    row.label.classList.toggle('hidden', !shown);
    row.value.classList.toggle('hidden', !shown);
  }
  if (!shown) return;
  // Compare against what was written last: the DOM holds the translation.
  if (row.labelSource !== label) row.label.textContent = row.labelSource = label;
  if (row.valueSource !== value) row.value.textContent = row.valueSource = value;
}
const qualityNumber = (value, unit = '', digits = 0) =>
  value === null ? '—' : `${value.toFixed(digits)}${unit}`;
function qualityBitrateText(kbps) {
  if (kbps === null) return '—';
  return kbps < 1000 ? `${Math.round(kbps)} kbps` : `${(kbps / 1000).toFixed(2)} Mbps`;
}
function renderQualityOverlay() {
  const q = connectionQuality,
    size = q.width && q.height ? `${q.width}×${q.height}` : '—';
  if (q.transport === 'webrtc_h264') {
    setQualityRow('rtt', '往返延迟', qualityNumber(q.rttMs, ' ms'));
    setQualityRow('bitrate', '接收码率', qualityBitrateText(q.bitrateKbps));
    setQualityRow('loss', '丢包率', qualityNumber(q.packetLossPct, '%', 1));
    setQualityRow('fps', '解码帧率', qualityNumber(q.fps));
    setQualityRow('jitter', '抖动缓冲', qualityNumber(q.jitterBufferMs, ' ms'));
    setQualityRow('adaptive', '自适应码率', adaptiveBitrateText());
  } else {
    setQualityRow('rtt', null);
    // The browser exposes no byte counts for a multipart <img>.
    setQualityRow('bitrate', '码率', '不可用');
    setQualityRow('loss', null);
    setQualityRow('fps', q.fpsSource === 'browser' ? '帧率（浏览器实测）' : '帧率（服务端采集）', qualityNumber(q.fps));
    setQualityRow('jitter', null);
    setQualityRow('adaptive', null);
  }
  setQualityRow('size', '分辨率', size);
}
function resetConnectionQuality(transport) {
  Object.assign(connectionQuality, {
    transport,
    updatedAt: 0,
    rttMs: null,
    bitrateKbps: null,
    packetLossPct: null,
    fps: null,
    fpsSource: null,
    jitterBufferMs: null,
    width: null,
    height: null,
  });
  qualityBaseline = null;
}
const statsCounter = (value) => (Number.isFinite(value) ? value : null);
// Growth of one cumulative counter between samples; null when unknown or when the
// counter went backwards (the stream was reset, the delta means nothing).
function counterDelta(now, before) {
  now = statsCounter(now);
  before = statsCounter(before);
  return now === null || before === null || now < before ? null : now - before;
}
function readWebrtcStats(report) {
  let inbound = null,
    transport = null,
    nominated = null;
  const byId = new Map();
  report.forEach((entry) => {
    byId.set(entry.id, entry);
    if (entry.type === 'inbound-rtp' && (entry.kind || entry.mediaType) === 'video') inbound = entry;
    else if (entry.type === 'transport') transport = transport || entry;
    else if (entry.type === 'candidate-pair' && (entry.selected || (entry.nominated && entry.state === 'succeeded')))
      nominated = nominated || entry;
  });
  const pair = byId.get(transport?.selectedCandidatePairId) || nominated;
  return { inbound, rtt: statsCounter(pair?.currentRoundTripTime) };
}
function applyWebrtcStats(report) {
  const q = connectionQuality,
    { inbound, rtt } = readWebrtcStats(report);
  q.rttMs = rtt === null ? null : rtt * 1000;
  q.fpsSource = 'decoder';
  if (!inbound) {
    Object.assign(q, { bitrateKbps: null, packetLossPct: null, fps: null, jitterBufferMs: null });
    qualityBaseline = null;
    return;
  }
  q.width = statsCounter(inbound.frameWidth) ?? (webrtcFeed.videoWidth || null);
  q.height = statsCounter(inbound.frameHeight) ?? (webrtcFeed.videoHeight || null);
  const before = qualityBaseline,
    seconds = before && before.id === inbound.id ? (inbound.timestamp - before.timestamp) / 1000 : null;
  // A browser may serve a cached report: same timestamp, keep the last rates.
  if (seconds === 0) return;
  qualityBaseline = inbound;
  if (!(seconds > 0)) {
    Object.assign(q, { bitrateKbps: null, packetLossPct: null, fps: null, jitterBufferMs: null });
    return;
  }
  const bytes = counterDelta(inbound.bytesReceived, before.bytesReceived),
    received = counterDelta(inbound.packetsReceived, before.packetsReceived),
    frames = counterDelta(inbound.framesDecoded, before.framesDecoded),
    delay = counterDelta(inbound.jitterBufferDelay, before.jitterBufferDelay),
    emitted = counterDelta(inbound.jitterBufferEmittedCount, before.jitterBufferEmittedCount);
  // packetsLost may legitimately shrink when late packets arrive.
  const lostNow = statsCounter(inbound.packetsLost),
    lostBefore = statsCounter(before.packetsLost),
    lost = lostNow === null || lostBefore === null ? null : Math.max(0, lostNow - lostBefore);
  q.bitrateKbps = bytes === null ? null : (bytes * 8) / seconds / 1000;
  q.packetLossPct = received === null || lost === null || received + lost === 0 ? null : (lost / (received + lost)) * 100;
  q.fps = frames === null ? null : frames / seconds;
  q.jitterBufferMs = delay === null || !emitted ? null : (delay / emitted) * 1000;
}
function applyMjpegStats() {
  const q = connectionQuality,
    now = performance.now(),
    before = qualityBaseline;
  qualityBaseline = { at: now, loads: shared.mjpegLoadEvents };
  q.width = feed.naturalWidth || null;
  q.height = feed.naturalHeight || null;
  if (shared.mjpegLoadEvents >= MJPEG_MIN_LOADS_TO_TRUST) {
    q.fpsSource = 'browser';
    const loads = before ? counterDelta(shared.mjpegLoadEvents, before.loads) : null;
    if (before && now > before.at) q.fps = loads === null ? null : (loads * 1000) / (now - before.at);
  } else {
    const reported = Number(shared.latestVideoStatus?.frames_per_second);
    q.fpsSource = 'server';
    q.fps = shared.latestVideoStatus?.frames_per_second != null && Number.isFinite(reported) ? reported : null;
  }
}
async function sampleQuality(run) {
  if (shared.activeVideoTransport === 'webrtc_h264') {
    const peer = shared.webrtcPeer;
    if (peer?.connectionState === 'disconnected') resetConnectionQuality('webrtc_h264');
    else if (peer) {
      try {
        const report = await peer.getStats();
        if (run !== qualityRun || peer !== shared.webrtcPeer) return;
        applyWebrtcStats(report);
      } catch (_) {
        if (run !== qualityRun) return;
      }
    }
  } else applyMjpegStats();
  connectionQuality.updatedAt = Date.now();
  if (shared.activeVideoTransport === 'webrtc_h264') adaptiveBitrateTick();
  renderQualityOverlay();
  qualityTimer = setTimeout(() => sampleQuality(run), QUALITY_INTERVAL_MS);
}
function qualityShouldRun() {
  return (
    (qualityEnabled || adaptiveBitrateWantsStats()) &&
    !document.hidden &&
    shouldVideoRun() &&
    shared.videoUiState === 'playing' &&
    shared.latestVideoStatus?.state === 'ready' &&
    (shared.activeVideoTransport === 'webrtc_h264' || shared.activeVideoTransport === 'mjpeg')
  );
}
// Called on every video state change; does nothing unless the answer changed.
function syncQualityOverlay() {
  const transport = qualityShouldRun() ? shared.activeVideoTransport : null;
  $('#quality-overlay').classList.toggle('hidden', !transport || !qualityEnabled);
  if (transport === qualityTransport) return;
  qualityTransport = transport;
  clearTimeout(qualityTimer);
  const run = ++qualityRun;
  resetConnectionQuality(transport || 'none');
  if (transport) sampleQuality(run);
}
function syncQualityButton() {
  $('#quality-toggle').setAttribute('aria-pressed', String(qualityEnabled));
}
function setQualityOverlayEnabled(enabled) {
  qualityEnabled = enabled;
  setSetting('quality_overlay', qualityEnabled);
  syncQualityButton();
  syncQualityOverlay();
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  for (const key of ['rtt', 'bitrate', 'loss', 'fps', 'jitter', 'adaptive', 'size']) {
    const label = document.createElement('dt'),
      value = document.createElement('dd');
    $('#quality-overlay').append(label, value);
    qualityRows[key] = { label, value };
  }
  $('#quality-toggle').addEventListener('click', () => setQualityOverlayEnabled(!qualityEnabled));
  document.addEventListener('visibilitychange', syncQualityOverlay);
  syncQualityButton();
}

export { connectionQuality, qualityBitrateText, renderQualityOverlay, setQualityOverlayEnabled, syncQualityOverlay };
