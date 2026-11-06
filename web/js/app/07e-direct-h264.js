import { shared } from './state.js';
import { directFeed } from './00-core.js';
import { resetAdaptiveBitrate } from './07c-adaptive-bitrate.js';

let active = null;
let serial = 0;
export function directH264Available() {
  return shared.bootstrap.capabilities?.video_direct_h264 === true && globalThis.isSecureContext === true
    && typeof globalThis.VideoDecoder === 'function' && typeof globalThis.EncodedVideoChunk === 'function';
}
export function stopDirectH264() {
  const run = active;
  active = null;
  shared.directSessionId = null;
  if (!run) return;
  clearTimeout(run.timer);
  run.socket.onclose = null;
  run.socket.onerror = null;
  run.socket.onmessage = null;
  run.socket.close();
  if (run.decoder?.state !== 'closed') run.decoder?.close();
  clearTimeout(run.pendingBitrate?.timeout);
  run.pendingBitrate?.reject(new Error('Direct H.264 已断开'));
  run.pendingBitrate = null;
}
export function setDirectBitrate(kbps) {
  const run = active;
  if (!run || run.socket.readyState !== WebSocket.OPEN) return Promise.reject(new Error('Direct H.264 未连接'));
  if (run.pendingBitrate) return Promise.reject(new Error('正在调整 Direct H.264 码率'));
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      if (run.pendingBitrate?.timeout === timeout) run.pendingBitrate = null;
      reject(new Error('Direct H.264 码率调整超时'));
    }, 5000);
    run.pendingBitrate = { resolve, reject, timeout };
    run.socket.send(JSON.stringify({ type: 'bitrate', kbps }));
  });
}
export function readDirectStats() {
  const run = active;
  if (!run) return null;
  const now = performance.now(), before = run.sample;
  run.sample = { at: now, bytes: run.bytes, frames: run.frames };
  const seconds = before ? (now - before.at) / 1000 : 0;
  return {
    fresh: seconds > 0 && run.frames > before.frames,
    bitrateKbps: seconds > 0 ? (run.bytes - before.bytes) * 8 / seconds / 1000 : null,
    fps: seconds > 0 ? (run.frames - before.frames) / seconds : null,
    rttMs: run.rtt, jitterBufferMs: run.delay, packetLossPct: null,
    width: directFeed.width, height: directFeed.height, fpsSource: 'decoder',
    effectiveKbps: run.effectiveKbps,
    decodeQueue: run.pending.size,
    framesDropped: run.dropped,
    freezeCount: run.freezes,
    decoder: `WebCodecs · ${run.config?.codec || 'H.264'}`,
  };
}
export function startDirectH264({ onPlaying, onFailure, onResize }) {
  stopDirectH264();
  const socket = new WebSocket(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/api/video/h264/ws`);
  socket.binaryType = 'arraybuffer';
  const run = { socket, decoder: null, timer: 0, epoch: 0, pending: new Map(), needsKey: true,
    bytes: 0, frames: 0, offset: Infinity, rtt: null, delay: null, sample: null, pendingBitrate: null,
    id: `direct-${++serial}`, lastSequence: 0, effectiveKbps: null,
    dropped: 0, freezes: 0, displayedAt: null };
  active = run;
  shared.directSessionId = run.id;
  const current = () => active === run;
  const fail = (message) => {
    if (!current()) return;
    stopDirectH264();
    onFailure(message);
  };
  const watchdog = () => {
    clearTimeout(run.timer);
    run.timer = setTimeout(() => fail('Direct H.264 等待画面超时'), 10000);
  };
  const ack = (sequence) => {
    if (current() && socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'ack', sequence }));
  };
  const makeDecoder = (config) => {
    const decoder = new VideoDecoder({
      output(frame) {
        try {
          if (!current() || decoder !== run.decoder) return;
          const sequence = run.pending.get(frame.timestamp);
          run.pending.delete(frame.timestamp);
          if (directFeed.width !== frame.displayWidth || directFeed.height !== frame.displayHeight) {
            directFeed.width = frame.displayWidth; directFeed.height = frame.displayHeight;
            onResize();
          }
          directFeed.getContext('2d', { alpha: false, desynchronized: true }).drawImage(frame, 0, 0);
          run.delay = Math.max(0, performance.now() - frame.timestamp / 1000 - run.offset);
          run.frames++;
          const now = performance.now();
          if (run.displayedAt !== null && now - run.displayedAt > 500) run.freezes++;
          run.displayedAt = now;
          watchdog();
          if (run.frames === 1) onPlaying();
          if (sequence !== undefined) ack(sequence);
        } catch (error) { fail(error.message); }
        finally { frame.close(); }
      },
      error(error) { fail(`Direct H.264 解码失败：${error.message}`); },
    });
    decoder.configure(config);
    return decoder;
  };
  let processing = Promise.resolve();
  socket.onmessage = (event) => {
    // The server allows at most eight unacknowledged frames, bounding this
    // queue even while isConfigSupported() is pending or the decoder is slow.
    processing = processing.then(async () => {
      if (!current()) return;
      if (typeof event.data === 'string') {
        const message = JSON.parse(event.data);
        if (message.type === 'config') {
          if (message.version !== 1 || !/^avc1\.[0-9a-f]{6}$/i.test(message.codec)) throw new Error('不支持的 Direct H.264 配置');
          const config = { codec: message.codec, optimizeForLatency: true, hardwareAcceleration: 'no-preference' };
          const supported = await VideoDecoder.isConfigSupported(config);
          if (!current()) return;
          if (!supported.supported) throw new Error('当前浏览器无法解码此 H.264 配置');
          ack(run.lastSequence);
          run.decoder?.close();
          run.pending.clear(); run.needsKey = true; run.epoch = message.epoch;
          run.offset = Infinity; run.delay = null; run.rtt = null; run.sample = null;
          run.decoder = makeDecoder(config);
          run.config = config;
          run.effectiveKbps = message.bitrate_kbps;
          if (!run.configured) {
            run.configured = true;
            resetAdaptiveBitrate(run.id, message.max_kbps, message.min_kbps, 'direct_h264');
          }
        } else if (message.type === 'bitrate_applied') run.effectiveKbps = message.kbps;
        else if (message.type === 'feedback') run.rtt = Number.isFinite(message.rtt_ms) ? message.rtt_ms : null;
        else if (message.type === 'bitrate' && run.pendingBitrate) {
          clearTimeout(run.pendingBitrate.timeout);
          run.pendingBitrate.resolve(message); run.pendingBitrate = null;
        }
        return;
      }
      const bytes = event.data;
      if (!(bytes instanceof ArrayBuffer) || bytes.byteLength < 33 || bytes.byteLength > 4 * 1024 * 1024 + 32) throw new Error('无效的 Direct H.264 帧');
      const view = new DataView(bytes);
      if (view.getUint32(0) !== 0x574b3236 || view.getUint32(8) !== run.epoch || !run.decoder) throw new Error('Direct H.264 帧版本不匹配');
      const key = (view.getUint8(4) & 1) !== 0, timestamp = Number(view.getBigUint64(12)), sequence = Number(view.getBigUint64(24));
      if (!Number.isSafeInteger(timestamp) || !Number.isSafeInteger(sequence) || sequence <= run.lastSequence) throw new Error('无效的 Direct H.264 时间戳');
      run.lastSequence = sequence;
      run.bytes += bytes.byteLength;
      run.offset = Math.min(run.offset, performance.now() - timestamp / 1000);
      if (run.decoder.decodeQueueSize > 4) {
        run.dropped += run.pending.size;
        run.decoder.close(); run.pending.clear(); run.needsKey = true;
        run.decoder = makeDecoder(run.config);
      }
      if (run.needsKey && !key) { run.dropped++; ack(sequence); return; }
      run.needsKey = false;
      run.pending.set(timestamp, sequence);
      run.decoder.decode(new EncodedVideoChunk({ type: key ? 'key' : 'delta', timestamp,
        duration: view.getUint32(20), data: new Uint8Array(bytes, 32) }));
    }).catch(error => fail(error.message));
  };
  socket.onerror = () => fail('Direct H.264 连接失败');
  socket.onclose = () => fail('Direct H.264 连接已断开');
  watchdog();
}
